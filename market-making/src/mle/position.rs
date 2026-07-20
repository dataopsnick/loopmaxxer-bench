//! MLE Position Size Inference
//!
//! Finds the optimal position size q* that maximizes the expected
//! log-likelihood using grid search + golden-section optimization.

use crate::gmm::model::GmmModel;
use crate::memorydb::vector_store::FeatureVector;
use crate::mle::likelihood::{LikelihoodParams, PositionLikelihood};
use serde::Serialize;

/// Result of the MLE position inference.
#[derive(Debug, Clone, Serialize)]
pub struct MlePositionResult {
    pub optimal_q: f64,
    pub max_log_likelihood: f64,
    pub likelihood_curve: Vec<(f64, f64)>,
    pub pi_noise: f64,
    pub pi_institutional: f64,
    pub pi_informed: f64,
    pub expected_spread_revenue: f64,
    pub expected_adverse_selection: f64,
    pub expected_sofr_carry: f64,
    pub expected_inventory_risk: f64,
}

/// MLE position size inferer using grid search + golden-section refinement.
pub struct MlePositionInferer {
    likelihood: PositionLikelihood,
    q_min: f64,
    q_max: f64,
    n_grid: usize,
}

impl MlePositionInferer {
    pub fn new(params: LikelihoodParams, q_min: f64, q_max: f64, n_grid: usize) -> Self {
        Self {
            likelihood: PositionLikelihood::new(params),
            q_min,
            q_max,
            n_grid,
        }
    }

    /// Find the MLE position size q* that maximizes expected log-likelihood.
    pub fn infer(&self, gmm: &GmmModel, features: &[FeatureVector]) -> MlePositionResult {
        let curve = self
            .likelihood
            .likelihood_curve(gmm, features, self.q_min, self.q_max, self.n_grid);

        let (best_q, _best_ll) = curve
            .iter()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .copied()
            .unwrap_or((0.0, f64::NEG_INFINITY));

        let refined = self.golden_section_search(gmm, features, best_q, self.grid_step());

        let optimal_q = refined.0;
        let max_ll = refined.1;

        let (spread_rev, adverse, sofr_carry, inv_risk) =
            self.compute_components(optimal_q, gmm, features);

        MlePositionResult {
            optimal_q,
            max_log_likelihood: max_ll,
            likelihood_curve: curve,
            pi_noise: gmm.pi_noise(),
            pi_institutional: gmm.pi_institutional(),
            pi_informed: gmm.pi_informed(),
            expected_spread_revenue: spread_rev,
            expected_adverse_selection: adverse,
            expected_sofr_carry: sofr_carry,
            expected_inventory_risk: inv_risk,
        }
    }

    fn grid_step(&self) -> f64 {
        if self.n_grid > 1 {
            (self.q_max - self.q_min) / (self.n_grid - 1) as f64
        } else {
            1.0
        }
    }

    fn golden_section_search(
        &self,
        gmm: &GmmModel,
        features: &[FeatureVector],
        center: f64,
        width: f64,
    ) -> (f64, f64) {
        let phi = 0.618033988749895;
        let inv_phi = 1.0 / (1.0 + phi);

        let mut a = center - width;
        let mut b = center + width;

        if a < self.q_min {
            a = self.q_min;
        }
        if b > self.q_max {
            b = self.q_max;
        }

        let mut c = b - (b - a) * inv_phi;
        let mut d = a + (b - a) * inv_phi;

        let mut fc = self.likelihood.log_likelihood(c, gmm, features);
        let mut fd = self.likelihood.log_likelihood(d, gmm, features);

        for _ in 0..50 {
            if (b - a).abs() < 1e-4 {
                break;
            }

            if fc > fd {
                b = d;
                d = c;
                fd = fc;
                c = b - (b - a) * inv_phi;
                fc = self.likelihood.log_likelihood(c, gmm, features);
            } else {
                a = c;
                c = d;
                fc = fd;
                d = a + (b - a) * inv_phi;
                fd = self.likelihood.log_likelihood(d, gmm, features);
            }
        }

        let mid = (a + b) / 2.0;
        let ll_mid = self.likelihood.log_likelihood(mid, gmm, features);
        (mid, ll_mid)
    }

    fn compute_components(
        &self,
        q: f64,
        gmm: &GmmModel,
        features: &[FeatureVector],
    ) -> (f64, f64, f64, f64) {
        if features.is_empty() {
            return (0.0, 0.0, 0.0, 0.0);
        }

        let pi_noise = gmm.pi_noise();
        let pi_informed = gmm.pi_informed();

        let p = self.likelihood.params();
        let spread = p.half_spread * 2.0;
        let fill_rate = p.liquidity_kappa * pi_noise;
        let spread_rev = fill_rate * spread * q.abs() / (1.0 + q.abs() * 0.001);

        let avg_informed_flow = features
            .iter()
            .map(|f| f.signed_order_flow.abs() * pi_informed)
            .sum::<f64>()
            / features.len() as f64;

        let avg_ret_pred = features
            .iter()
            .map(|f| f.return_predictability.abs())
            .sum::<f64>()
            / features.len() as f64;

        let adverse = avg_informed_flow * avg_ret_pred * q.abs() * p.spot_price;

        let sofr_carry = self.likelihood.sofr_controller().sofr_financing_cost(
            q,
            p.spot_price,
            p.margin_haircut,
            p.borrow_premium,
        ) * p.time_to_horizon;

        let vol = features.iter().map(|f| f.vol_atm).sum::<f64>() / features.len() as f64;
        let inv_risk = p.risk_aversion_gamma * vol.powi(2) * q.powi(2) * p.time_to_horizon;

        (spread_rev, adverse, sofr_carry, inv_risk)
    }
}

impl Default for MlePositionInferer {
    fn default() -> Self {
        Self::new(LikelihoodParams::default(), -5000.0, 5000.0, 200)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_finds_optimal_position() {
        let inferer =
            MlePositionInferer::new(LikelihoodParams::default(), -5000.0, 5000.0, 100);
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

        let result = inferer.infer(&gmm, &features);
        assert!(result.optimal_q.is_finite(), "Optimal q should be finite");
        assert!(
            result.max_log_likelihood.is_finite(),
            "Max LL should be finite"
        );
        assert!(
            !result.likelihood_curve.is_empty(),
            "Curve should be non-empty"
        );
    }
}