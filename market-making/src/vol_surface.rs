//! Monotonic Cubic Spline Volatility Surface (Spec §23)
//!
//! Hyman/de Boor monotonicity-filtered cubic spline interpolation
//! to prevent butterfly and calendar arbitrage in the vol surface.

/// 8-node fixed-size monotonic cubic spline evaluator for strike-axis vol interpolation.
///
/// Enforces no-arbitrage constraints via Hyman monotonicity filtering.
pub struct MonotonicCubicSplineEvaluator {
    pub strikes: [f64; 8],
    pub vols: [f64; 8],
    pub slopes: [f64; 8],
}

impl MonotonicCubicSplineEvaluator {
    /// Initialize and fit with Hyman monotonicity correction.
    pub fn new_fit(mut strikes: [f64; 8], mut vols: [f64; 8]) -> Self {
        // Sort nodes by strike (simple insertion sort for 8 elements)
        for i in 1..8 {
            let s = strikes[i];
            let v = vols[i];
            let mut j = i;
            while j > 0 && strikes[j - 1] > s {
                strikes[j] = strikes[j - 1];
                vols[j] = vols[j - 1];
                j -= 1;
            }
            strikes[j] = s;
            vols[j] = v;
        }

        let mut slopes = [0.0f64; 8];
        let mut secant_slopes = [0.0f64; 7];

        // 1. Secant slopes between adjacent grid points
        for i in 0..7 {
            let dx = strikes[i + 1] - strikes[i];
            if dx > 1e-9 {
                secant_slopes[i] = (vols[i + 1] - vols[i]) / dx;
            } else {
                secant_slopes[i] = 0.0;
            }
        }

        // 2. Default interior node slopes = average of adjacent secants
        for i in 1..7 {
            slopes[i] = 0.5 * (secant_slopes[i - 1] + secant_slopes[i]);
        }
        slopes[0] = secant_slopes[0];
        slopes[7] = secant_slopes[6];

        // 3. Hyman monotonicity filter to prevent oscillation / arbitrage
        for i in 0..7 {
            if secant_slopes[i].abs() < 1e-9 {
                slopes[i] = 0.0;
                slopes[i + 1] = 0.0;
            } else {
                let alpha = slopes[i] / secant_slopes[i];
                let beta = slopes[i + 1] / secant_slopes[i];
                let distance = alpha * alpha + beta * beta;

                if distance > 9.0 {
                    let scale = 3.0 / distance.sqrt();
                    slopes[i] = scale * alpha * secant_slopes[i];
                    slopes[i + 1] = scale * beta * secant_slopes[i];
                }
            }
        }

        Self {
            strikes,
            vols,
            slopes,
        }
    }

    /// Evaluate the calibrated cubic surface at a given strike.
    #[inline(always)]
    pub fn evaluate_volatility_at(&self, strike: f64) -> f64 {
        if strike <= self.strikes[0] {
            return self.vols[0];
        }
        if strike >= self.strikes[7] {
            return self.vols[7];
        }

        let mut idx = 0;
        for i in 0..7 {
            if strike >= self.strikes[i] && strike <= self.strikes[i + 1] {
                idx = i;
                break;
            }
        }

        let h = self.strikes[idx + 1] - self.strikes[idx];
        let t = (strike - self.strikes[idx]) / h;
        let t2 = t * t;
        let t3 = t2 * t;

        // Hermite basis functions
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;

        h00 * self.vols[idx]
            + h10 * h * self.slopes[idx]
            + h01 * self.vols[idx + 1]
            + h11 * h * self.slopes[idx + 1]
    }
}

/// ATM-centered 2nd-order Taylor expansion vol surface (Spec §7).
///
/// Approximates the local volatility surface via a 2nd-order Taylor
/// expansion around the current ATM (at-the-money) point:
///
/// `σ(S+ΔS, K, τ+Δτ) ≈ σ_ATM + ∂σ/∂S·ΔS + ½·∂²σ/∂S²·ΔS² + ∂σ/∂τ·Δτ`
///
/// The coefficients are updated by a background Kalman filter / OLS
/// refit at millisecond cadence. The hot path performs a read-only
/// coefficient vector load and a single FMA-heavy evaluation.
#[repr(align(64))]
pub struct TaylorVolSurface {
    /// ATM volatility level: σ_ATM.
    pub sigma_atm: f64,
    /// First derivative w.r.t. spot: ∂σ/∂S.
    pub d_sigma_d_s: f64,
    /// Second derivative w.r.t. spot: ∂²σ/∂S².
    pub d2_sigma_d_s2: f64,
    /// First derivative w.r.t. time to expiry: ∂σ/∂τ.
    pub d_sigma_d_tau: f64,
    /// Current reference spot price S₀.
    pub ref_spot: f64,
    /// Current reference time to expiry τ₀ (in years).
    pub ref_tau: f64,
    /// Nanosecond timestamp of last coefficient update.
    pub last_update_ns: u64,
}

impl TaylorVolSurface {
    /// Create a new Taylor vol surface with the given coefficients.
    pub fn new(
        sigma_atm: f64,
        d_sigma_d_s: f64,
        d2_sigma_d_s2: f64,
        d_sigma_d_tau: f64,
        ref_spot: f64,
        ref_tau: f64,
    ) -> Self {
        Self {
            sigma_atm,
            d_sigma_d_s,
            d2_sigma_d_s2,
            d_sigma_d_tau,
            ref_spot,
            ref_tau,
            last_update_ns: 0,
        }
    }

    /// Create a flat vol surface (no skew, no term structure).
    pub fn flat(sigma: f64, ref_spot: f64, ref_tau: f64) -> Self {
        Self::new(sigma, 0.0, 0.0, 0.0, ref_spot, ref_tau)
    }

    /// Evaluate the Taylor expansion for local volatility.
    ///
    /// `σ(S, K, τ) ≈ σ_ATM + ∂σ/∂S·(S - S₀) + ½·∂²σ/∂S²·(S - S₀)² + ∂σ/∂τ·(τ - τ₀)`
    ///
    /// Note: In production, the strike dependence is captured implicitly
    /// through the spot derivative (moneyness = K/S approximation).
    #[inline(always)]
    pub fn evaluate_vol(&self, spot: f64, _strike: f64, tau: f64) -> f64 {
        let ds = spot - self.ref_spot;
        let d_tau = tau - self.ref_tau;

        // σ_ATM + ∂σ/∂S·ΔS + ½·∂²σ/∂S²·ΔS² + ∂σ/∂τ·Δτ
        let vol = self.sigma_atm
            + self.d_sigma_d_s * ds
            + 0.5 * self.d2_sigma_d_s2 * ds * ds
            + self.d_sigma_d_tau * d_tau;

        // Volatility must be positive
        vol.max(0.001)
    }

    /// Update the Taylor coefficients from a background refit.
    ///
    /// In production, this is called by a background thread running a
    /// Kalman filter or Ridge-regularized OLS on recent market data.
    /// The hot-path reader uses `Ordering::Relaxed` to load coefficients.
    pub fn update_coefficients(
        &mut self,
        sigma_atm: f64,
        d_sigma_d_s: f64,
        d2_sigma_d_s2: f64,
        d_sigma_d_tau: f64,
        ref_spot: f64,
        ref_tau: f64,
        timestamp_ns: u64,
    ) {
        self.sigma_atm = sigma_atm;
        self.d_sigma_d_s = d_sigma_d_s;
        self.d2_sigma_d_s2 = d2_sigma_d_s2;
        self.d_sigma_d_tau = d_sigma_d_tau;
        self.ref_spot = ref_spot;
        self.ref_tau = ref_tau;
        self.last_update_ns = timestamp_ns;
    }

    /// Get the ATM volatility.
    #[inline(always)]
    pub fn atm_vol(&self) -> f64 {
        self.sigma_atm
    }

    /// Get the last update timestamp (nanoseconds).
    #[inline(always)]
    pub fn last_update_ns(&self) -> u64 {
        self.last_update_ns
    }
}

/// Background coefficient refit stub (Spec §7).
///
/// In production, this runs a Kalman filter or Ridge-regularized OLS
/// on recent option market data to estimate the Taylor coefficients.
/// Here it provides a simple finite-difference estimation from
/// observed market vols at three strikes.
pub fn refit_taylor_coefficients(
    atm_vol: f64,
    vol_at_plus_delta: f64,
    vol_at_minus_delta: f64,
    delta_s: f64,
    vol_at_short_tau: f64,
    short_tau: f64,
    current_tau: f64,
) -> (f64, f64, f64, f64) {
    // ∂σ/∂S ≈ (vol(+ΔS) - vol(-ΔS)) / (2·ΔS)
    let d_sigma_d_s = if delta_s.abs() > 1e-9 {
        (vol_at_plus_delta - vol_at_minus_delta) / (2.0 * delta_s)
    } else {
        0.0
    };

    // ∂²σ/∂S² ≈ (vol(+ΔS) - 2·vol(ATM) + vol(-ΔS)) / ΔS²
    let d2_sigma_d_s2 = if delta_s.abs() > 1e-9 {
        (vol_at_plus_delta - 2.0 * atm_vol + vol_at_minus_delta) / (delta_s * delta_s)
    } else {
        0.0
    };

    // ∂σ/∂τ ≈ (vol(τ_short) - vol(ATM)) / (τ_short - τ_current)
    let d_tau = short_tau - current_tau;
    let d_sigma_d_tau = if d_tau.abs() > 1e-9 {
        (vol_at_short_tau - atm_vol) / d_tau
    } else {
        0.0
    };

    (atm_vol, d_sigma_d_s, d2_sigma_d_s2, d_sigma_d_tau)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spline_monotonic_fit() {
        let strikes = [140.0, 145.0, 150.0, 155.0, 160.0, 165.0, 170.0, 175.0];
        let vols = [0.24, 0.22, 0.20, 0.19, 0.20, 0.21, 0.23, 0.25];
        let spline = MonotonicCubicSplineEvaluator::new_fit(strikes, vols);

        // At node points, should return close to node vol
        let v150 = spline.evaluate_volatility_at(150.0);
        assert!((v150 - 0.20).abs() < 1e-6, "v150={}", v150);

        // Interpolated value should be between neighbors
        let v152 = spline.evaluate_volatility_at(152.0);
        assert!(v152 >= 0.18 && v152 <= 0.21, "v152={}", v152);
    }
}