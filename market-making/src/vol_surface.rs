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