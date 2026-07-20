//! SOFR Hedge Controller (Spec §3, §5.3)
//!
//! Whalley-Wilmott no-transaction band with SOFR drift correction
//! for optimal delta-hedge timing under carry-cost constraints.

/// Per-asset hedge parameters.
#[derive(Debug, Clone)]
pub struct AssetHedgeParameters {
    pub volatility: f64,
    pub gamma: f64,
    pub theta: f64,
    pub half_spread: f64,
    pub sofr_borrow_premium: f64,
}

impl Default for AssetHedgeParameters {
    fn default() -> Self {
        Self {
            volatility: 0.18,
            gamma: 0.04,
            theta: -0.02,
            half_spread: 0.01,
            sofr_borrow_premium: 0.0025,
        }
    }
}

/// SOFR-biased hedge controller implementing Whalley-Wilmott bands.
pub struct SOFRHedgeController {
    pub risk_aversion_gamma: f64,
    pub sofr_base_rate: f64,
}

impl SOFRHedgeController {
    pub fn new(risk_aversion: f64, sofr: f64) -> Self {
        Self {
            risk_aversion_gamma: risk_aversion,
            sofr_base_rate: sofr,
        }
    }

    /// Evaluate whether a delta hedge should be executed.
    ///
    /// Returns `Some(hedge_qty)` if the imbalance exceeds the adjusted
    /// Whalley-Wilmott band, or `None` if within the no-transaction zone.
    #[inline(always)]
    pub fn evaluate_delta_hedge(
        &self,
        current_delta: f64,
        target_delta: f64,
        params: &AssetHedgeParameters,
        spot_price: f64,
        time_to_midnight: f64,
    ) -> Option<f64> {
        let delta_imbalance = current_delta - target_delta;

        // 1. Pure Whalley-Wilmott no-transaction band width
        let base_width = (1.5
            * (self.risk_aversion_gamma * params.gamma.powi(2) * params.half_spread)
            / (spot_price * params.volatility.powi(2)))
        .powf(1.0 / 3.0);

        // 2. Cumulative intraday SOFR financing cost for holding the imbalance
        let sofr_cost_per_day = if delta_imbalance > 0.0 {
            delta_imbalance * spot_price * (self.sofr_base_rate + params.sofr_borrow_premium)
        } else {
            delta_imbalance.abs() * spot_price * (self.sofr_base_rate - params.sofr_borrow_premium)
        };

        let cumulative_sofr_capital_loss = sofr_cost_per_day * time_to_midnight;

        // 3. Direct slippage cost of crossing the spread to hedge now
        let direct_slippage_cost = delta_imbalance.abs() * params.half_spread;

        // 4. SOFR-drift-adjusted band boundary
        let sofr_drift_factor = if direct_slippage_cost > 1e-9 {
            cumulative_sofr_capital_loss / direct_slippage_cost
        } else {
            0.0
        };
        let adjusted_threshold = base_width * (1.0 + sofr_drift_factor);

        if delta_imbalance.abs() > adjusted_threshold {
            Some(-delta_imbalance)
        } else {
            None
        }
    }

    /// Compute the SOFR financing cost function Φ_SOFR(q) for a position vector.
    ///
    /// Φ_SOFR(q) = Σ [ q_i * S_i * R_finance_i(q_i) + H_i(q) * SOFR ]
    #[inline(always)]
    pub fn sofr_financing_cost(
        &self,
        position: f64,
        spot_price: f64,
        margin_haircut: f64,
        borrow_premium: f64,
    ) -> f64 {
        let financing_rate = if position > 0.0 {
            self.sofr_base_rate + borrow_premium
        } else {
            -(self.sofr_base_rate - borrow_premium)
        };

        let carry_cost = position * spot_price * financing_rate;
        let margin_cost = position.abs() * spot_price * margin_haircut * self.sofr_base_rate;

        carry_cost + margin_cost
    }

    /// Compute the SOFR-biased Avellaneda-Stoikov reservation price.
    ///
    /// R_i = S_i - risk_aversion_penalty - SOFR_capital_carry_penalty
    #[inline(always)]
    pub fn reservation_price(
        &self,
        mid_price: f64,
        position: f64,
        volatility: f64,
        _spot_price: f64,
        time_to_horizon: f64,
        margin_haircut: f64,
        borrow_premium: f64,
    ) -> f64 {
        // Standard risk aversion penalty
        let risk_penalty =
            position * self.risk_aversion_gamma * volatility.powi(2) * time_to_horizon;

        // SOFR capital carry penalty (marginal cost of holding one more unit)
        let sofr_penalty = position.signum()
            * (self.sofr_base_rate + margin_haircut + borrow_premium)
            * time_to_horizon;

        mid_price - risk_penalty - sofr_penalty
    }
}

impl Default for SOFRHedgeController {
    fn default() -> Self {
        Self::new(0.01, 0.0535)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hedge_within_band() {
        let controller = SOFRHedgeController::new(0.01, 0.0535);
        let params = AssetHedgeParameters::default();
        let result = controller.evaluate_delta_hedge(0.5, 0.0, &params, 150.0, 0.45);
        assert!(result.is_none(), "Small imbalance should not trigger hedge");
    }

    #[test]
    fn hedge_outside_band() {
        let controller = SOFRHedgeController::new(0.01, 0.0535);
        let params = AssetHedgeParameters::default();
        let result = controller.evaluate_delta_hedge(10000.0, 0.0, &params, 150.0, 0.45);
        assert!(result.is_some(), "Large imbalance should trigger hedge");
        assert!(result.unwrap() < 0.0, "Hedge should reduce position");
    }

    #[test]
    fn reservation_price_skew() {
        let controller = SOFRHedgeController::new(0.01, 0.0535);
        let r_long = controller.reservation_price(150.0, 1000.0, 0.2, 150.0, 0.45, 0.15, 0.0025);
        let r_short = controller.reservation_price(150.0, -1000.0, 0.2, 150.0, 0.45, 0.15, 0.0025);
        assert!(r_long < 150.0, "Long position should lower reservation price");
        assert!(r_short > 150.0, "Short position should raise reservation price");
    }
}