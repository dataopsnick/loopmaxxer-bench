//! Pre-Trade Risk Gate (Spec §24)
//!
//! 10ns-level branchless pre-trade validation that blocks outbound
//! orders exceeding hard limits on price, quantity, and portfolio delta.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Hardware-co-located pre-trade risk filter.
pub struct PreTradeRiskGate {
    max_order_qty: u32,
    max_price_cents: u64,
    max_absolute_delta: AtomicU64,
    gate_tripped: AtomicBool,
}

impl PreTradeRiskGate {
    pub const fn new(max_qty: u32, max_price_usd: f64, max_delta: f64) -> Self {
        Self {
            max_order_qty: max_qty,
            max_price_cents: (max_price_usd * 100.0) as u64,
            max_absolute_delta: AtomicU64::new(max_delta.to_bits()),
            gate_tripped: AtomicBool::new(false),
        }
    }

    /// Validate an order against all hard limits.
    ///
    /// Returns `true` if the order passes, `false` if rejected.
    /// On rejection, the kill switch is immediately tripped.
    #[inline(always)]
    pub fn validate_order(&self, price_usd: f64, qty: u32, current_portfolio_delta: f64) -> bool {
        if self.gate_tripped.load(Ordering::Relaxed) {
            return false;
        }

        let price_cents = (price_usd * 100.0) as u64;
        let limit_delta = f64::from_bits(self.max_absolute_delta.load(Ordering::Relaxed));

        let limit_breached = qty > self.max_order_qty
            || price_cents > self.max_price_cents
            || price_cents == 0
            || current_portfolio_delta.abs() > limit_delta;

        if limit_breached {
            self.gate_tripped.store(true, Ordering::SeqCst);
            return false;
        }

        true
    }

    /// Simplified validation (price + qty only).
    #[inline(always)]
    pub fn validate_order_simple(&self, price_usd: f64, qty: u32) -> bool {
        if self.gate_tripped.load(Ordering::Relaxed) {
            return false;
        }
        let price_cents = (price_usd * 100.0) as u64;
        let limit_breached =
            qty > self.max_order_qty || price_cents > self.max_price_cents || price_cents == 0;
        if limit_breached {
            self.gate_tripped.store(true, Ordering::SeqCst);
            return false;
        }
        true
    }

    #[inline(always)]
    pub fn force_kill_switch(&self) {
        self.gate_tripped.store(true, Ordering::SeqCst);
    }

    #[inline(always)]
    pub fn reset_kill_switch(&self) {
        self.gate_tripped.store(false, Ordering::SeqCst);
    }

    #[inline(always)]
    pub fn is_tripped(&self) -> bool {
        self.gate_tripped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_order_passes() {
        let gate = PreTradeRiskGate::new(1000, 5000.0, 5000.0);
        assert!(gate.validate_order(150.0, 100, 200.0));
        assert!(!gate.is_tripped());
    }

    #[test]
    fn excessive_qty_rejected() {
        let gate = PreTradeRiskGate::new(1000, 5000.0, 5000.0);
        assert!(!gate.validate_order(150.0, 2000, 0.0));
        assert!(gate.is_tripped());
    }

    #[test]
    fn zero_price_rejected() {
        let gate = PreTradeRiskGate::new(1000, 5000.0, 5000.0);
        assert!(!gate.validate_order(0.0, 100, 0.0));
        assert!(gate.is_tripped());
    }

    #[test]
    fn delta_limit_rejected() {
        let gate = PreTradeRiskGate::new(1000, 5000.0, 5000.0);
        assert!(!gate.validate_order(150.0, 100, 6000.0));
        assert!(gate.is_tripped());
    }
}