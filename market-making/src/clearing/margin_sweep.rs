//! EOD Margin Sweep & Excess Margin Deployment (Spec §6)
//!
//! At end-of-day, the clearing engine sweeps excess margin to minimize
//! SPAN/TIMS haircut and deploys surplus capital to bilateral repo
//! or SOFR overnight deposits for carry optimization.

/// The destination for an excess margin sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepDestination {
    /// Bilateral repo (SOFR-collateralized overnight lending).
    BilateralRepo,
    /// SOFR overnight reverse repo deposit at the Fed.
    SofrOvernightDeposit,
    /// Retain at clearing broker (no sweep).
    ClearingBroker,
}

/// A margin sweep action.
#[derive(Debug, Clone)]
pub struct SweepAction {
    /// Amount to sweep (USD).
    pub amount: f64,
    /// Destination of the sweep.
    pub destination: SweepDestination,
    /// Estimated overnight carry (USD).
    pub estimated_carry: f64,
}

/// The margin sweep engine.
///
/// Computes the optimal EOD margin sweep to minimize haircut and
/// maximize overnight carry on excess capital.
pub struct MarginSweepEngine {
    /// Current posted margin at clearing broker (USD).
    posted_margin: f64,
    /// Required margin after netting (USD).
    required_margin: f64,
    /// SOFR overnight rate (e.g., 0.0535 for 5.35%).
    sofr_rate: f64,
    /// Bilateral repo rate (typically SOFR - 1-2 bps).
    repo_rate: f64,
    /// Minimum excess margin threshold before sweeping (USD).
    sweep_threshold: f64,
}

impl MarginSweepEngine {
    /// Create a new margin sweep engine.
    pub fn new(
        posted_margin: f64,
        required_margin: f64,
        sofr_rate: f64,
        repo_rate: f64,
        sweep_threshold: f64,
    ) -> Self {
        Self {
            posted_margin,
            required_margin,
            sofr_rate,
            repo_rate,
            sweep_threshold,
        }
    }

    /// Compute the excess margin available for sweep.
    ///
    /// Excess = posted - required (if positive).
    #[inline(always)]
    pub fn excess_margin(&self) -> f64 {
        (self.posted_margin - self.required_margin).max(0.0)
    }

    /// Compute the optimal sweep actions for EOD.
    ///
    /// Returns a list of sweep actions that deploy excess margin to
    /// the highest-yielding overnight destination while maintaining
    /// a buffer at the clearing broker.
    pub fn compute_eod_sweep(&self) -> Vec<SweepAction> {
        let excess = self.excess_margin();

        if excess < self.sweep_threshold {
            // Not enough excess to justify sweeping
            return vec![SweepAction {
                amount: excess,
                destination: SweepDestination::ClearingBroker,
                estimated_carry: 0.0,
            }];
        }

        let mut actions = Vec::new();

        // Keep a buffer at the clearing broker (10% of excess or threshold, whichever is smaller)
        let buffer = (excess * 0.1).min(self.sweep_threshold);
        if buffer > 0.0 {
            actions.push(SweepAction {
                amount: buffer,
                destination: SweepDestination::ClearingBroker,
                estimated_carry: 0.0,
            });
        }

        let sweepable = excess - buffer;

        // Split between bilateral repo and SOFR overnight deposit
        // based on which offers better net rate
        if self.repo_rate >= self.sofr_rate {
            // Repo rate is better or equal — deploy all to repo
            let carry = sweepable * self.repo_rate / 365.0;
            actions.push(SweepAction {
                amount: sweepable,
                destination: SweepDestination::BilateralRepo,
                estimated_carry: carry,
            });
        } else {
            // SOFR deposit is better — deploy all to SOFR overnight
            let carry = sweepable * self.sofr_rate / 365.0;
            actions.push(SweepAction {
                amount: sweepable,
                destination: SweepDestination::SofrOvernightDeposit,
                estimated_carry: carry,
            });
        }

        actions
    }

    /// Get the total estimated overnight carry from all sweep actions.
    pub fn total_overnight_carry(&self) -> f64 {
        self.compute_eod_sweep()
            .iter()
            .map(|a| a.estimated_carry)
            .sum()
    }

    /// Update posted margin (e.g., after additional collateral posted).
    pub fn set_posted_margin(&mut self, margin: f64) {
        self.posted_margin = margin;
    }

    /// Update required margin (e.g., after position netting).
    pub fn set_required_margin(&mut self, margin: f64) {
        self.required_margin = margin;
    }

    /// Get the posted margin.
    #[inline(always)]
    pub fn posted_margin(&self) -> f64 {
        self.posted_margin
    }

    /// Get the required margin.
    #[inline(always)]
    pub fn required_margin(&self) -> f64 {
        self.required_margin
    }

    /// Get the SOFR rate.
    #[inline(always)]
    pub fn sofr_rate(&self) -> f64 {
        self.sofr_rate
    }

    /// Get the repo rate.
    #[inline(always)]
    pub fn repo_rate(&self) -> f64 {
        self.repo_rate
    }

    /// EOD clearing broker API integration stub.
    ///
    /// In production, this would submit the sweep instructions to the
    /// clearing broker's API. Here it returns a formatted instruction string.
    pub fn submit_sweep_instructions(&self) -> String {
        let actions = self.compute_eod_sweep();
        let mut instructions = String::new();

        for (i, action) in actions.iter().enumerate() {
            let dest = match action.destination {
                SweepDestination::BilateralRepo => "BILATERAL_REPO",
                SweepDestination::SofrOvernightDeposit => "SOFR_ONIGHT_DEPOSIT",
                SweepDestination::ClearingBroker => "CLEARING_BROKER_RETAIN",
            };
            instructions.push_str(&format!(
                "SWEEP[{}]: ${:.2} -> {} (carry: ${:.4})\n",
                i, action.amount, dest, action.estimated_carry
            ));
        }

        instructions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excess_margin_positive() {
        let engine = MarginSweepEngine::new(1_000_000.0, 800_000.0, 0.0535, 0.0525, 50_000.0);
        assert!((engine.excess_margin() - 200_000.0).abs() < 1e-9);
    }

    #[test]
    fn excess_margin_zero_when_deficit() {
        let engine = MarginSweepEngine::new(500_000.0, 800_000.0, 0.0535, 0.0525, 50_000.0);
        assert!((engine.excess_margin() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn no_sweep_below_threshold() {
        let engine = MarginSweepEngine::new(810_000.0, 800_000.0, 0.0535, 0.0525, 50_000.0);
        let actions = engine.compute_eod_sweep();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].destination, SweepDestination::ClearingBroker);
        assert!((actions[0].amount - 10_000.0).abs() < 1e-9);
    }

    #[test]
    fn sweep_to_repo_when_better_rate() {
        let engine = MarginSweepEngine::new(1_000_000.0, 800_000.0, 0.0535, 0.0545, 50_000.0);
        let actions = engine.compute_eod_sweep();
        assert!(actions.len() >= 2);

        // Should have at least one bilateral repo action
        let repo_action = actions
            .iter()
            .find(|a| a.destination == SweepDestination::BilateralRepo);
        assert!(repo_action.is_some(), "Should sweep to bilateral repo when rate is better");

        let repo = repo_action.unwrap();
        assert!(repo.amount > 0.0);
        assert!(repo.estimated_carry > 0.0);
    }

    #[test]
    fn sweep_to_sofr_when_better_rate() {
        let engine = MarginSweepEngine::new(1_000_000.0, 800_000.0, 0.0535, 0.0525, 50_000.0);
        let actions = engine.compute_eod_sweep();
        assert!(actions.len() >= 2);

        // Should have at least one SOFR deposit action
        let sofr_action = actions
            .iter()
            .find(|a| a.destination == SweepDestination::SofrOvernightDeposit);
        assert!(sofr_action.is_some(), "Should sweep to SOFR deposit when rate is better");
    }

    #[test]
    fn total_overnight_carry_positive() {
        let engine = MarginSweepEngine::new(1_000_000.0, 800_000.0, 0.0535, 0.0525, 50_000.0);
        let carry = engine.total_overnight_carry();
        assert!(carry > 0.0, "Overnight carry should be positive: {}", carry);
    }

    #[test]
    fn submit_sweep_instructions_format() {
        let engine = MarginSweepEngine::new(1_000_000.0, 800_000.0, 0.0535, 0.0525, 50_000.0);
        let instructions = engine.submit_sweep_instructions();
        assert!(instructions.contains("SWEEP"));
        assert!(instructions.contains("SOFR_ONIGHT_DEPOSIT"));
    }

    #[test]
    fn update_margins() {
        let mut engine = MarginSweepEngine::new(1_000_000.0, 800_000.0, 0.0535, 0.0525, 50_000.0);
        engine.set_posted_margin(1_200_000.0);
        engine.set_required_margin(700_000.0);
        assert!((engine.excess_margin() - 500_000.0).abs() < 1e-9);
    }
}