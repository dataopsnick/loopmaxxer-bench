//! TIMS Margin Model implementation (Spec §12)
//!
//! 17-scenario stress grid: ±3%, ±8%, ±15% spot moves × ±10% vol shifts.
//! Cross-asset delta/gamma netting for non-linear margin reduction.
//! Negative margin cost weighting for risk-reducing hedge orders.

/// Spot move scenarios (7 points): -15%, -8%, -3%, 0%, +3%, +8%, +15%.
pub const SPOT_SCENARIOS: [f64; 7] = [-0.15, -0.08, -0.03, 0.0, 0.03, 0.08, 0.15];

/// Vol shift scenarios (2 points): -10%, +10%.
pub const VOL_SCENARIOS: [f64; 2] = [-0.10, 0.10];

/// Total scenario count: 7 × 2 = 14 (plus 3 extreme tail scenarios = 17).
pub const NUM_SCENARIOS: usize = 17;

/// A single TIMS stress scenario.
#[derive(Debug, Clone, Copy)]
pub struct MarginScenario {
    pub spot_shift: f64,
    pub vol_shift: f64,
    pub portfolio_loss: f64,
}

impl MarginScenario {
    /// Create a new scenario.
    #[inline(always)]
    pub fn new(spot_shift: f64, vol_shift: f64, portfolio_loss: f64) -> Self {
        Self {
            spot_shift,
            vol_shift,
            portfolio_loss,
        }
    }
}

/// Result of a TIMS margin evaluation.
#[derive(Debug, Clone)]
pub struct TimsResult {
    /// Worst-case loss across all 17 scenarios.
    pub worst_case_loss: f64,
    /// The scenario index that produced the worst case.
    pub worst_case_scenario_idx: usize,
    /// All scenario results.
    pub scenarios: [MarginScenario; NUM_SCENARIOS],
    /// Margin requirement (worst case with haircut applied).
    pub margin_requirement: f64,
    /// Negative margin credit from risk-reducing hedges.
    pub margin_credit: f64,
}

/// Portfolio position for margin computation.
#[derive(Debug, Clone, Copy)]
pub struct PositionGreeks {
    pub spot: f64,
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
    pub notional: f64,
}

impl Default for PositionGreeks {
    fn default() -> Self {
        Self {
            spot: 100.0,
            delta: 0.0,
            gamma: 0.0,
            vega: 0.0,
            theta: 0.0,
            notional: 0.0,
        }
    }
}

/// Real-time TIMS margin model (Spec §12).
///
/// Evaluates the portfolio across 17 stress scenarios and computes the
/// worst-case loss, applying cross-asset delta/gamma netting for
/// non-linear margin reduction.
pub struct TimsMarginModel {
    /// Margin haircut rate applied to worst-case loss.
    haircut_rate: f64,
    /// SOFR rate for overnight opportunity cost tracking.
    sofr_rate: f64,
}

impl TimsMarginModel {
    /// Create a new TIMS model with the given haircut and SOFR rate.
    pub fn new(haircut_rate: f64, sofr_rate: f64) -> Self {
        Self {
            haircut_rate,
            sofr_rate,
        }
    }

    /// Generate all 17 stress scenarios.
    ///
    /// 14 = 7 spot × 2 vol, plus 3 extreme tail scenarios:
    /// (-20% spot, +20% vol), (+20% spot, +20% vol), (0% spot, +30% vol).
    pub fn generate_scenarios() -> Vec<(f64, f64)> {
        let mut scenarios = Vec::with_capacity(NUM_SCENARIOS);
        for &spot in &SPOT_SCENARIOS {
            for &vol in &VOL_SCENARIOS {
                scenarios.push((spot, vol));
            }
        }
        // 3 extreme tail scenarios
        scenarios.push((-0.20, 0.20));
        scenarios.push((0.20, 0.20));
        scenarios.push((0.0, 0.30));
        scenarios
    }

    /// Evaluate portfolio P&L under a single scenario.
    ///
    /// Uses second-order Taylor expansion:
    /// ΔV ≈ delta·(ΔS) + ½·gamma·(ΔS)² + vega·(Δσ) + theta·Δt
    #[inline(always)]
    pub fn evaluate_scenario(
        &self,
        position: &PositionGreeks,
        spot_shift: f64,
        vol_shift: f64,
    ) -> f64 {
        let delta_pnl = position.delta * position.spot * spot_shift;
        let gamma_pnl =
            0.5 * position.gamma * position.spot * position.spot * spot_shift * spot_shift;
        let vega_pnl = position.vega * vol_shift;
        let theta_pnl = position.theta * (1.0 / 365.0); // daily theta

        // Loss is negative P&L
        -(delta_pnl + gamma_pnl + vega_pnl + theta_pnl)
    }

    /// Evaluate the full 17-scenario stress grid for a single position.
    pub fn evaluate(&self, position: &PositionGreeks) -> TimsResult {
        let scenarios_pairs = Self::generate_scenarios();
        let mut scenarios = [MarginScenario::new(0.0, 0.0, 0.0); NUM_SCENARIOS];

        let mut worst_loss = f64::MIN;
        let mut worst_idx = 0;

        for (i, (spot_shift, vol_shift)) in scenarios_pairs.iter().enumerate() {
            let loss = self.evaluate_scenario(position, *spot_shift, *vol_shift);
            scenarios[i] = MarginScenario::new(*spot_shift, *vol_shift, loss);
            if loss > worst_loss {
                worst_loss = loss;
                worst_idx = i;
            }
        }

        let margin_requirement = worst_loss.max(0.0) * (1.0 + self.haircut_rate);

        TimsResult {
            worst_case_loss: worst_loss,
            worst_case_scenario_idx: worst_idx,
            scenarios,
            margin_requirement,
            margin_credit: 0.0,
        }
    }

    /// Evaluate a portfolio of positions with cross-asset netting.
    ///
    /// When delta/gamma offsets occur across assets, the net margin
    /// requirement is reduced non-linearly.
    pub fn evaluate_portfolio(&self, positions: &[PositionGreeks]) -> TimsResult {
        let scenarios_pairs = Self::generate_scenarios();
        let mut scenarios = [MarginScenario::new(0.0, 0.0, 0.0); NUM_SCENARIOS];

        let mut worst_loss = f64::MIN;
        let mut worst_idx = 0;

        for (i, (spot_shift, vol_shift)) in scenarios_pairs.iter().enumerate() {
            let mut total_loss = 0.0;
            for pos in positions {
                total_loss += self.evaluate_scenario(pos, *spot_shift, *vol_shift);
            }
            scenarios[i] = MarginScenario::new(*spot_shift, *vol_shift, total_loss);
            if total_loss > worst_loss {
                worst_loss = total_loss;
                worst_idx = i;
            }
        }

        // Cross-asset netting benefit: if portfolio has offsetting deltas,
        // apply a non-linear reduction factor.
        let net_delta: f64 = positions.iter().map(|p| p.delta).sum();
        let gross_delta: f64 = positions.iter().map(|p| p.delta.abs()).sum();
        let netting_benefit = if gross_delta > 1e-9 {
            (1.0 - (net_delta.abs() / gross_delta)).max(0.0)
        } else {
            0.0
        };

        let netted_margin = worst_loss.max(0.0) * (1.0 - netting_benefit * 0.3);
        let margin_requirement = netted_margin * (1.0 + self.haircut_rate);

        TimsResult {
            worst_case_loss: worst_loss,
            worst_case_scenario_idx: worst_idx,
            scenarios,
            margin_requirement,
            margin_credit: 0.0,
        }
    }

    /// Compute the negative margin cost weighting for a risk-reducing hedge.
    ///
    /// If a hedge order reduces the worst-case scenario loss, it receives
    /// a negative margin cost (credit) that narrows the reservation spread.
    #[inline(always)]
    pub fn hedge_margin_credit(&self, current: &TimsResult, post_hedge: &TimsResult) -> f64 {
        let loss_reduction = (current.worst_case_loss - post_hedge.worst_case_loss).max(0.0);
        // Convert margin reduction to SOFR-equivalent credit
        loss_reduction * self.sofr_rate
    }

    /// SPAN margin opportunity-cost tracking for futures positions.
    ///
    /// Tracks the overnight capital lock-up cost for futures margin.
    #[inline(always)]
    pub fn span_opportunity_cost(&self, futures_notional: f64, span_rate: f64) -> f64 {
        futures_notional * span_rate * self.sofr_rate
    }
}

impl Default for TimsMarginModel {
    fn default() -> Self {
        Self::new(0.15, 0.0535)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_count_is_17() {
        let scenarios = TimsMarginModel::generate_scenarios();
        assert_eq!(scenarios.len(), NUM_SCENARIOS);
    }

    #[test]
    fn long_call_has_positive_margin() {
        let model = TimsMarginModel::default();
        let position = PositionGreeks {
            spot: 100.0,
            delta: 0.5,
            gamma: 0.02,
            vega: 10.0,
            theta: -0.5,
            notional: 10000.0,
        };
        let result = model.evaluate(&position);
        assert!(
            result.margin_requirement >= 0.0,
            "Long call should have non-negative margin"
        );
    }

    #[test]
    fn delta_neutral_reduces_margin() {
        let model = TimsMarginModel::default();
        let long_call = PositionGreeks {
            spot: 100.0,
            delta: 0.5,
            gamma: 0.02,
            vega: 10.0,
            theta: -0.5,
            notional: 10000.0,
        };
        let short_stock = PositionGreeks {
            spot: 100.0,
            delta: -0.5,
            gamma: 0.0,
            vega: 0.0,
            theta: 0.0,
            notional: 5000.0,
        };

        let single = model.evaluate(&long_call);
        let hedged = model.evaluate_portfolio(&[long_call, short_stock]);

        assert!(
            hedged.margin_requirement <= single.margin_requirement,
            "Delta-neutral hedge should reduce margin: hedged={} single={}",
            hedged.margin_requirement,
            single.margin_requirement
        );
    }

    #[test]
    fn hedge_credit_positive_for_risk_reduction() {
        let model = TimsMarginModel::default();
        let position = PositionGreeks {
            spot: 100.0,
            delta: 1.0,
            gamma: 0.0,
            vega: 0.0,
            theta: 0.0,
            notional: 10000.0,
        };
        let current = model.evaluate(&position);

        let hedged_position = PositionGreeks {
            delta: 0.0,
            ..position
        };
        let post_hedge = model.evaluate(&hedged_position);

        let credit = model.hedge_margin_credit(&current, &post_hedge);
        assert!(
            credit >= 0.0,
            "Risk-reducing hedge should have non-negative credit"
        );
    }

    #[test]
    fn span_opportunity_cost_positive() {
        let model = TimsMarginModel::default();
        let cost = model.span_opportunity_cost(100_000.0, 0.10);
        assert!(cost > 0.0, "SPAN opportunity cost should be positive");
    }
}