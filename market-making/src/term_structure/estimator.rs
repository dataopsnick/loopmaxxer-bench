//! Term-Structure Beta Estimator implementation (Spec §14)
//!
//! β_term = Cov(ΔP_t, ΔP_{t+1}) / Var(ΔP_t)
//!
//! Uses EWMA covariance/variance accumulators with atomic CAS dual-slot
//! updates for lock-free concurrent updates.

use std::sync::atomic::{AtomicU64, Ordering};

/// Term-structure regime classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Regime {
    /// Normal contango: near-month < next-month, β_term stable.
    Contango,
    /// Backwardation reversal: β_term collapsed below threshold.
    Backwardation,
}

impl Regime {
    /// Returns true if the regime is a backwardation alert.
    #[inline(always)]
    pub fn is_backwardation(self) -> bool {
        matches!(self, Regime::Backwardation)
    }
}

/// Overnight SOFR financing avoidance target tracker via cross-expiry
/// beta covariance estimation (Spec §14).
///
/// Tracks EWMA covariance between near-month (ΔP_t) and next-month (ΔP_{t+1})
/// futures price changes, and computes the real-time term-structure beta.
pub struct TermStructureBetaEstimator {
    covariance_accumulator: AtomicU64,
    variance_accumulator: AtomicU64,
    /// Exponential decay factor (e.g. 0.9992).
    decay_alpha: f64,
    /// β_term threshold below which we flag a backwardation regime.
    backwardation_threshold: f64,
}

impl TermStructureBetaEstimator {
    /// Create a new estimator with the given EWMA decay factor.
    pub fn new(decay_alpha: f64) -> Self {
        Self {
            covariance_accumulator: AtomicU64::new(0.0f64.to_bits()),
            variance_accumulator: AtomicU64::new(0.0f64.to_bits()),
            decay_alpha,
            backwardation_threshold: 0.5,
        }
    }

    /// Create with a custom backwardation β_term threshold.
    pub fn with_threshold(decay_alpha: f64, threshold: f64) -> Self {
        Self {
            covariance_accumulator: AtomicU64::new(0.0f64.to_bits()),
            variance_accumulator: AtomicU64::new(0.0f64.to_bits()),
            decay_alpha,
            backwardation_threshold: threshold,
        }
    }

    /// Receive per-second near-month/next-month price change history and
    /// refresh the EWMA term-structure covariance (Spec §14).
    ///
    /// Returns the current β_term estimate.
    #[inline(always)]
    pub fn update_term_metrics(&self, d_prompt: f64, d_next: f64) -> f64 {
        let mut cov_bits = self.covariance_accumulator.load(Ordering::Relaxed);
        let mut var_bits = self.variance_accumulator.load(Ordering::Relaxed);

        let mut next_cov;
        let mut next_var;

        loop {
            let prev_cov = f64::from_bits(cov_bits);
            let prev_var = f64::from_bits(var_bits);

            next_cov =
                self.decay_alpha * prev_cov + (1.0 - self.decay_alpha) * (d_prompt * d_next);
            next_var =
                self.decay_alpha * prev_var + (1.0 - self.decay_alpha) * (d_prompt * d_prompt);

            // Dual atomic-slot CAS contention resolution.
            match self.covariance_accumulator.compare_exchange_weak(
                cov_bits,
                next_cov.to_bits(),
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let _ = self.variance_accumulator.compare_exchange_weak(
                        var_bits,
                        next_var.to_bits(),
                        Ordering::SeqCst,
                        Ordering::Relaxed,
                    );
                    break;
                }
                Err(actual) => {
                    cov_bits = actual;
                    var_bits = self.variance_accumulator.load(Ordering::Relaxed);
                }
            }
        }

        if next_var > 1e-9 {
            next_cov / next_var // Real-time beta output
        } else {
            1.0 // Default forced correlation
        }
    }

    /// Load the current β_term estimate without updating state.
    #[inline(always)]
    pub fn current_beta(&self) -> f64 {
        let cov = f64::from_bits(self.covariance_accumulator.load(Ordering::Acquire));
        let var = f64::from_bits(self.variance_accumulator.load(Ordering::Acquire));
        if var > 1e-9 {
            cov / var
        } else {
            1.0
        }
    }

    /// Classify the current term-structure regime.
    #[inline(always)]
    pub fn current_regime(&self) -> Regime {
        if self.current_beta() < self.backwardation_threshold {
            Regime::Backwardation
        } else {
            Regime::Contango
        }
    }

    /// Compute the inventory risk penalty widening factor for backwardation.
    ///
    /// In contango: factor = 1.0 (no widening).
    /// In backwardation: factor > 1.0, scaling with how far β_term has fallen.
    #[inline(always)]
    pub fn inventory_penalty_widening(&self) -> f64 {
        let beta = self.current_beta();
        if beta < self.backwardation_threshold {
            // Widen penalty proportionally to the collapse depth.
            1.0 + (self.backwardation_threshold - beta).max(0.0) * 2.0
        } else {
            1.0
        }
    }
}

impl Default for TermStructureBetaEstimator {
    fn default() -> Self {
        Self::new(0.9995)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beta_converges_to_one_for_correlated_moves() {
        let est = TermStructureBetaEstimator::new(0.9);
        // Identical moves → beta should converge to 1.0
        for _ in 0..1000 {
            est.update_term_metrics(1.0, 1.0);
        }
        let beta = est.current_beta();
        assert!(
            (beta - 1.0).abs() < 0.05,
            "beta should be ~1.0 for perfectly correlated moves, got {}",
            beta
        );
    }

    #[test]
    fn beta_zero_for_uncorrelated_moves() {
        let est = TermStructureBetaEstimator::new(0.9);
        // Alternating signs → covariance ~0
        for i in 0..1000 {
            let d_prompt = if i % 2 == 0 { 1.0 } else { -1.0 };
            let d_next = 1.0; // constant next-month
            est.update_term_metrics(d_prompt, d_next);
        }
        let beta = est.current_beta();
        assert!(
            beta.abs() < 0.1,
            "beta should be ~0 for uncorrelated moves, got {}",
            beta
        );
    }

    #[test]
    fn backwardation_regime_detected() {
        let est = TermStructureBetaEstimator::with_threshold(0.9, 0.5);
        // Opposite-sign moves → negative beta → backwardation
        for _ in 0..1000 {
            est.update_term_metrics(1.0, -1.0);
        }
        assert_eq!(est.current_regime(), Regime::Backwardation);
        assert!(est.inventory_penalty_widening() > 1.0);
    }

    #[test]
    fn contango_regime_stable() {
        let est = TermStructureBetaEstimator::with_threshold(0.9, 0.5);
        for _ in 0..1000 {
            est.update_term_metrics(1.0, 1.0);
        }
        assert_eq!(est.current_regime(), Regime::Contango);
        assert!((est.inventory_penalty_widening() - 1.0).abs() < 1e-9);
    }
}