//! Ultra-Fast Pricer (Spec §13)
//!
//! Hastings rational approximation for the normal CDF and a
//! minimal Black-Scholes analytical engine optimized for FMA.

/// AVX-512-friendly low-latency transcendental and analytical option pricer.
pub struct UltraFastPricer;

impl UltraFastPricer {
    /// High-precision CDF approximation using Hastings/Cody-Waite rational polynomial.
    ///
    /// Avoids division and trig calls; uses only register-level multiply-add (FMA).
    #[inline(always)]
    pub fn fast_normal_cdf(x: f64) -> f64 {
        if x < -6.0 {
            return 0.0;
        }
        if x > 6.0 {
            return 1.0;
        }

        let abs_x = x.abs();
        let p = 0.2316419;
        let t = 1.0 / (1.0 + p * abs_x);

        let a1 = 0.319381530;
        let a2 = -0.356563782;
        let a3 = 1.781477937;
        let a4 = -1.821255978;
        let a5 = 1.330274429;

        // Horner's method for FMA optimization
        let polynomial = t * (a1 + t * (a2 + t * (a3 + t * (a4 + t * a5))));

        // e^(-x^2 / 2) accelerated
        let exponent = -0.5 * abs_x * abs_x;
        let l_density = 0.3989422804014327 * exponent.exp();

        let cdf_abs = 1.0 - l_density * polynomial;

        if x >= 0.0 {
            cdf_abs
        } else {
            1.0 - cdf_abs
        }
    }

    /// Standard normal PDF.
    #[inline(always)]
    pub fn fast_normal_pdf(x: f64) -> f64 {
        0.3989422804014327 * (-0.5 * x * x).exp()
    }

    /// Low-latency Black-Scholes price, delta, and gamma computation.
    ///
    /// Returns `(price, delta, gamma)`.
    #[inline(always)]
    pub fn calculate_option_theoreticals(
        spot: f64,
        strike: f64,
        time_to_expiry: f64,
        volatility: f64,
        rate: f64,
        is_call: bool,
    ) -> (f64, f64, f64) {
        if time_to_expiry <= 0.0001 {
            let price = if is_call {
                (spot - strike).max(0.0)
            } else {
                (strike - spot).max(0.0)
            };
            let delta = if is_call {
                if spot > strike { 1.0 } else { 0.0 }
            } else if spot < strike {
                -1.0
            } else {
                0.0
            };
            return (price, delta, 0.0);
        }

        let sqrt_t = time_to_expiry.sqrt();
        let vol_sq = volatility * volatility;

        let ln_s_k = (spot / strike).ln();

        let d1 = (ln_s_k + (rate + 0.5 * vol_sq) * time_to_expiry) / (volatility * sqrt_t);
        let d2 = d1 - volatility * sqrt_t;

        let n_d1 = Self::fast_normal_cdf(d1);
        let n_d2 = Self::fast_normal_cdf(d2);

        let exp_rt = (-rate * time_to_expiry).exp();

        let pdf_d1 = Self::fast_normal_pdf(d1);

        if is_call {
            let price = spot * n_d1 - strike * exp_rt * n_d2;
            let delta = n_d1;
            let gamma = pdf_d1 / (spot * volatility * sqrt_t);
            (price, delta, gamma)
        } else {
            let price = strike * exp_rt * (1.0 - n_d2) - spot * (1.0 - n_d1);
            let delta = n_d1 - 1.0;
            let gamma = pdf_d1 / (spot * volatility * sqrt_t);
            (price, delta, gamma)
        }
    }

    /// Compute vega (common for call and put).
    #[inline(always)]
    pub fn calculate_vega(
        spot: f64,
        strike: f64,
        time_to_expiry: f64,
        volatility: f64,
        rate: f64,
    ) -> f64 {
        if time_to_expiry <= 0.0001 {
            return 0.0;
        }
        let sqrt_t = time_to_expiry.sqrt();
        let vol_sq = volatility * volatility;
        let ln_s_k = (spot / strike).ln();
        let d1 = (ln_s_k + (rate + 0.5 * vol_sq) * time_to_expiry) / (volatility * sqrt_t);
        let pdf_d1 = Self::fast_normal_pdf(d1);
        pdf_d1 * spot * sqrt_t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdf_known_values() {
        assert!((UltraFastPricer::fast_normal_cdf(0.0) - 0.5).abs() < 1e-4);
        assert!((UltraFastPricer::fast_normal_cdf(1.96) - 0.975).abs() < 1e-3);
        assert!((UltraFastPricer::fast_normal_cdf(-1.96) - 0.025).abs() < 1e-3);
    }

    #[test]
    fn bs_call_price() {
        let (price, delta, _gamma) =
            UltraFastPricer::calculate_option_theoreticals(100.0, 100.0, 1.0, 0.2, 0.05, true);
        assert!((price - 10.4506).abs() < 0.1, "price={}", price);
        assert!((delta - 0.6368).abs() < 0.01, "delta={}", delta);
    }

    #[test]
    fn bs_put_price() {
        let (price, delta, _gamma) =
            UltraFastPricer::calculate_option_theoreticals(100.0, 100.0, 1.0, 0.2, 0.05, false);
        assert!((price - 5.5735).abs() < 0.1, "price={}", price);
        assert!((delta - (-0.3632)).abs() < 0.01, "delta={}", delta);
    }
}