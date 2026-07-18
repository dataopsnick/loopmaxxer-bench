 
//! Position Likelihood Construction //! //! Constructs the log-likelihood function for the delta-neutral market maker's //! position size q, incorporating: //! - Spread revenue (from noise-trader fills) //! - Adverse selection cost (from informed-trader flow) //! - SOFR carry cost (from holding inventory overnight)
use crate::gmm::model::GmmModel; use crate::memorydb::vector_store::FeatureVector; use crate::sofr::SOFRHedgeController;
/// Parameters for the position likelihood function. #[derive(Debug, Clone)] pub struct LikelihoodParams { pub spot_price: f64, pub sofr_rate: f64, pub margin_haircut: f64, pub borrow_premium: f64, pub liquidity_kappa: f64, pub risk_aversion_gamma: f64, pub time_to_horizon: f64, pub half_spread: f64, }
impl Default for LikelihoodParams { fn default() -> Self { Self { spot_price: 150.0, sofr_rate: 0.0535, margin_haircut: 0.15, borrow_premium: 0.0025, liquidity_kappa: 2.1, risk_aversion_gamma: 0.015, time_to_horizon: 0.45, half_spread: 0.01, } } }
/// Position likelihood evaluator. /// /// L(q) = E[spread_revenue(q)] - E[adverse_selection(q)] - Φ_SOFR(q) pub struct PositionLikelihood { params: LikelihoodParams, sofr_controller: SOFRHedgeController, }
impl PositionLikelihood { pub fn new(params: LikelihoodParams) -> Self { let sofr_controller = SOFRHedgeController::new(params.risk_aversion_gamma, params.sofr_rate); Self { params, sofr_controller, } }
/// Get the likelihood parameters.
pub fn params(&self) -> &LikelihoodParams {
    &self.params
}

/// Get the SOFR controller.
pub fn sofr_controller(&self) -> &SOFRHedgeController {
    &self.sofr_controller
}

/// Compute the expected log-likelihood for a given position size q.
pub fn log_likelihood(&self, q: f64, gmm: &GmmModel, features: &[FeatureVector]) -> f64 {
    if features.is_empty() {
        return 0.0;
    }

    let pi_noise = gmm.pi_noise();
    let pi_informed = gmm.pi_informed();

    // 1. Expected spread revenue
    let spread = self.params.half_spread * 2.0;
    let fill_rate = self.params.liquidity_kappa * pi_noise;
    let spread_revenue = fill_rate * spread * q.abs() / (1.0 + q.abs() * 0.001);

    // 2. Expected adverse selection cost
    let avg_informed_flow = features
        .iter()
        .map(|f| f.signed_order_flow.abs() * pi_informed)
        .sum::<f64>()
        / features.len() as f64;

    let avg_return_predictability = features
        .iter()
        .map(|f| f.return_predictability.abs())
        .sum::<f64>()
        / features.len() as f64;

    let adverse_selection =
        avg_informed_flow * avg_return_predictability * q.abs() * self.params.spot_price;

    // 3. SOFR carry cost
    let sofr_carry = self.sofr_controller.sofr_financing_cost(
        q,
        self.params.spot_price,
        self.params.margin_haircut,
        self.params.borrow_premium,
    ) * self.params.time_to_horizon;

    // 4. Inventory risk penalty (Avellaneda-Stoikov)
    let vol = features.iter().map(|f| f.vol_atm).sum::<f64>() / features.len() as f64;
    let inventory_risk =
        self.params.risk_aversion_gamma * vol.powi(2) * q.powi(2) * self.params.time_to_horizon;

    spread_revenue - adverse_selection - sofr_carry - inventory_risk
}

/// Compute the likelihood over a range of position sizes.
pub fn likelihood_curve(
    &self,
    gmm: &GmmModel,
    features: &[FeatureVector],
    q_min: f64,
    q_max: f64,
    n_points: usize,
) -> Vec<(f64, f64)> {
    if n_points == 0 {
        return Vec::new();
    }

    let step = if n_points > 1 {
        (q_max - q_min) / (n_points - 1) as f64
    } else {
        0.0
    };

    (0..n_points)
        .map(|i| {
            let q = q_min + step * i as f64;
            let ll = self.log_likelihood(q, gmm, features);
            (q, ll)
        })
        .collect()
}
}

#[cfg(test)] mod tests { use super::*; use crate::gmm::model::GmmModel;


#[test]
fn likelihood_at_zero() {
    let params = LikelihoodParams::default();
    let likelihood = PositionLikelihood::new(params);
    let gmm = GmmModel::new(6);
    let features = vec![];

    let ll = likelihood.log_likelihood(0.0, &gmm, &features);
    assert!((ll - 0.0).abs() < 1e-9, "Likelihood at q=0 should be 0: {}", ll);
}

#[test]
fn likelihood_curve_generated() {
    let params = LikelihoodParams::default();
    let likelihood = PositionLikelihood::new(params);
    let gmm = GmmModel::new(6);
    let features = vec![FeatureVector {
        timestamp_ns: 0,
        symbol: "AAPL".to_string(),
        normalized_trade_size: 0.01,
        signed_order_flow: 100.0,
        ofi_ewma: 0.001,
        spread_width: 0.02,
        vol_atm: 0.20,
        return_predictability: 0.1,
    }];

    let curve = likelihood.likelihood_curve(&gmm, &features, -1000.0, 1000.0, 50);
    assert_eq!(curve.len(), 50);
}
}
