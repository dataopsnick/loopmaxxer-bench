//! Order Flow Imbalance (OFI) Microstructure Drift (Spec §17)
//!
//! Real-time EWMA-filtered OFI estimator that produces a micro-price
//! drift correction α_drift for the reservation price.

/// Real-time OFI and micro-price drift estimator.
pub struct MicrostructureOFI {
    prev_bid_price: f64,
    prev_bid_size: f64,
    prev_ask_price: f64,
    prev_ask_size: f64,
    ofi_ema: f64,
    decay: f64,
    multiplier: f64,
}

impl MicrostructureOFI {
    pub fn new(decay: f64, multiplier: f64) -> Self {
        Self {
            prev_bid_price: 0.0,
            prev_bid_size: 0.0,
            prev_ask_price: 0.0,
            prev_ask_size: 0.0,
            ofi_ema: 0.0,
            decay,
            multiplier,
        }
    }

    /// Compute the drift adjustment α_drift from incoming quote updates.
    ///
    /// I_OFI(t) = ΔV_bid(t) - ΔV_ask(t)
    /// α_drift(t) = EWMA(I_OFI, λ) * θ
    #[inline(always)]
    pub fn compute_drift_adjustment(
        &mut self,
        bid_px: f64,
        bid_sz: f64,
        ask_px: f64,
        ask_sz: f64,
    ) -> f64 {
        let delta_v_bid = if bid_px > self.prev_bid_price {
            bid_sz
        } else if bid_px == self.prev_bid_price {
            bid_sz - self.prev_bid_size
        } else {
            0.0
        };

        let delta_v_ask = if ask_px < self.prev_ask_price {
            ask_sz
        } else if ask_px == self.prev_ask_price {
            ask_sz - self.prev_ask_size
        } else {
            0.0
        };

        let ofi_instant = delta_v_bid - delta_v_ask;
        self.ofi_ema = self.decay * self.ofi_ema + (1.0 - self.decay) * ofi_instant;

        self.prev_bid_price = bid_px;
        self.prev_bid_size = bid_sz;
        self.prev_ask_price = ask_px;
        self.prev_ask_size = ask_sz;

        self.ofi_ema * self.multiplier
    }

    /// Get the current EWMA OFI value (without multiplier).
    #[inline(always)]
    pub fn current_ofi(&self) -> f64 {
        self.ofi_ema
    }

    /// Reset the estimator state.
    pub fn reset(&mut self) {
        self.prev_bid_price = 0.0;
        self.prev_bid_size = 0.0;
        self.prev_ask_price = 0.0;
        self.prev_ask_size = 0.0;
        self.ofi_ema = 0.0;
    }
}

impl Default for MicrostructureOFI {
    fn default() -> Self {
        Self::new(0.95, 0.001)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ofi_bid_improvement() {
        let mut ofi = MicrostructureOFI::new(0.9, 0.01);
        ofi.compute_drift_adjustment(150.0, 100.0, 150.10, 100.0);
        let drift = ofi.compute_drift_adjustment(150.05, 200.0, 150.10, 100.0);
        assert!(drift > 0.0, "Bid improvement should produce positive drift: {}", drift);
    }

    #[test]
    fn ofi_ask_improvement() {
        let mut ofi = MicrostructureOFI::new(0.9, 0.01);
        ofi.compute_drift_adjustment(150.0, 100.0, 150.10, 100.0);
        let drift = ofi.compute_drift_adjustment(150.0, 100.0, 150.05, 200.0);
        assert!(drift < 0.0, "Ask improvement should produce negative drift: {}", drift);
    }
}