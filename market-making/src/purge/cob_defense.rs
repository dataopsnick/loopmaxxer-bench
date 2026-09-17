//! Complex Order Book (COB) Legging Defense (Spec §9.1)
//!
//! Ingests SpiderRock `msgspreadbookquote` and `synSpot` changes to detect
//! COB spread trades, then asymmetrically skews simple-book quotes to
//! defend against legging-out arbitrage from competing institutions.

/// A Complex Order Book (COB) spread quote from SpiderRock's `msgspreadbookquote`.
#[derive(Debug, Clone, Copy)]
pub struct CobSpreadQuote {
    /// Theoretical price of the spread.
    pub theoretical_price: f64,
    /// Bid price of the spread.
    pub bid_price: f64,
    /// Ask price of the spread.
    pub ask_price: f64,
    /// Bid size.
    pub bid_size: f64,
    /// Ask size.
    pub ask_size: f64,
    /// Synthetic spot (`synSpot`) at the time of the quote.
    pub syn_spot: f64,
}

impl CobSpreadQuote {
    /// Mid price of the spread.
    #[inline(always)]
    pub fn mid(&self) -> f64 {
        (self.bid_price + self.ask_price) / 2.0
    }

    /// Spread width.
    #[inline(always)]
    pub fn width(&self) -> f64 {
        self.ask_price - self.bid_price
    }
}

/// Skew adjustment to apply to simple-book quotes.
#[derive(Debug, Clone, Copy)]
pub struct SkewAdjustment {
    /// Bid price offset (negative = lower the bid).
    pub bid_offset: f64,
    /// Ask price offset (positive = raise the ask).
    pub ask_offset: f64,
    /// Whether legging-out risk is detected.
    pub legging_risk_detected: bool,
}

impl Default for SkewAdjustment {
    fn default() -> Self {
        Self {
            bid_offset: 0.0,
            ask_offset: 0.0,
            legging_risk_detected: false,
        }
    }
}

/// COB legging defense controller (Spec §9.1).
///
/// Monitors COB spread quotes and detects when a large spread trade
/// creates pressure on individual option legs. When detected, it
/// asymmetrically skews the simple-book quotes to prevent pick-off
/// by legging-out arbitrageurs.
pub struct CobDefenseController {
    /// Previous COB mid price (for detecting large trades).
    prev_cob_mid: f64,
    /// Previous synSpot.
    prev_syn_spot: f64,
    /// Threshold for detecting a significant COB trade (fraction of spread).
    trade_detection_threshold: f64,
    /// Skew magnitude multiplier.
    skew_multiplier: f64,
}

impl CobDefenseController {
    /// Create a new COB defense controller.
    ///
    /// `trade_detection_threshold` is the fraction of the COB spread
    /// that a mid-price move must exceed to trigger legging defense.
    pub fn new(trade_detection_threshold: f64, skew_multiplier: f64) -> Self {
        Self {
            prev_cob_mid: 0.0,
            prev_syn_spot: 0.0,
            trade_detection_threshold,
            skew_multiplier,
        }
    }

    /// Process an incoming COB spread quote and compute the skew adjustment.
    ///
    /// When a COB trade is detected (mid moves more than threshold × width),
    /// the controller computes an asymmetric skew that:
    /// - Lowers the bid if the spread traded on the bid side (sell pressure)
    /// - Raises the ask if the spread traded on the ask side (buy pressure)
    #[inline(always)]
    pub fn process_cob_quote(&mut self, quote: &CobSpreadQuote) -> SkewAdjustment {
        let current_mid = quote.mid();
        let width = quote.width();

        // Detect synSpot movement (underlying pressure)
        let spot_delta = quote.syn_spot - self.prev_syn_spot;

        // Detect COB mid movement
        let mid_delta = if self.prev_cob_mid > 0.0 {
            current_mid - self.prev_cob_mid
        } else {
            0.0
        };

        // Update state
        self.prev_cob_mid = current_mid;
        self.prev_syn_spot = quote.syn_spot;

        // Check if the mid move exceeds the trade detection threshold
        let threshold = width * self.trade_detection_threshold;
        if threshold < 1e-9 || mid_delta.abs() < threshold {
            return SkewAdjustment::default();
        }

        // Legging-out risk detected: the COB trade implies pressure on
        // individual legs. Skew quotes asymmetrically to defend.
        let skew_amount = mid_delta.abs() * self.skew_multiplier;

        // If mid dropped (sell pressure on spread), lower bid and raise ask
        // If mid rose (buy pressure on spread), raise ask and lower bid
        let (bid_offset, ask_offset) = if mid_delta < 0.0 {
            // Spread sold → legs are being shorted → lower bid to avoid
            // being picked off on the bid side
            (-skew_amount, skew_amount * 0.5)
        } else {
            // Spread bought → legs are being bought → raise ask to avoid
            // being picked off on the ask side
            (-skew_amount * 0.5, skew_amount)
        };

        // Also factor in spot delta for additional skew
        let spot_skew = spot_delta * self.skew_multiplier * 0.3;

        SkewAdjustment {
            bid_offset: bid_offset - spot_skew.abs() * spot_delta.signum(),
            ask_offset: ask_offset + spot_skew.abs() * spot_delta.signum(),
            legging_risk_detected: true,
        }
    }

    /// Reset the controller state.
    pub fn reset(&mut self) {
        self.prev_cob_mid = 0.0;
        self.prev_syn_spot = 0.0;
    }
}

impl Default for CobDefenseController {
    fn default() -> Self {
        Self::new(0.5, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_skew_on_first_quote() {
        let mut ctrl = CobDefenseController::default();
        let quote = CobSpreadQuote {
            theoretical_price: 5.0,
            bid_price: 4.95,
            ask_price: 5.05,
            bid_size: 100.0,
            ask_size: 100.0,
            syn_spot: 150.0,
        };
        let skew = ctrl.process_cob_quote(&quote);
        assert!(!skew.legging_risk_detected, "First quote should not trigger skew");
    }

    #[test]
    fn no_skew_for_small_move() {
        let mut ctrl = CobDefenseController::default();
        let q1 = CobSpreadQuote {
            theoretical_price: 5.0,
            bid_price: 4.95,
            ask_price: 5.05,
            bid_size: 100.0,
            ask_size: 100.0,
            syn_spot: 150.0,
        };
        ctrl.process_cob_quote(&q1);

        // Small move within threshold
        let q2 = CobSpreadQuote {
            theoretical_price: 5.01,
            bid_price: 4.96,
            ask_price: 5.06,
            bid_size: 100.0,
            ask_size: 100.0,
            syn_spot: 150.0,
        };
        let skew = ctrl.process_cob_quote(&q2);
        assert!(!skew.legging_risk_detected, "Small move should not trigger skew");
    }

    #[test]
    fn skew_on_large_buy_side_trade() {
        let mut ctrl = CobDefenseController::default();
        let q1 = CobSpreadQuote {
            theoretical_price: 5.0,
            bid_price: 4.95,
            ask_price: 5.05,
            bid_size: 100.0,
            ask_size: 100.0,
            syn_spot: 150.0,
        };
        ctrl.process_cob_quote(&q1);

        // Large buy-side trade: mid jumps up significantly
        let q2 = CobSpreadQuote {
            theoretical_price: 5.5,
            bid_price: 5.45,
            ask_price: 5.55,
            bid_size: 50.0,
            ask_size: 200.0,
            syn_spot: 151.0,
        };
        let skew = ctrl.process_cob_quote(&q2);
        assert!(skew.legging_risk_detected, "Large buy should trigger skew");
        assert!(skew.ask_offset > 0.0, "Ask should be raised on buy pressure");
    }

    #[test]
    fn skew_on_large_sell_side_trade() {
        let mut ctrl = CobDefenseController::default();
        let q1 = CobSpreadQuote {
            theoretical_price: 5.0,
            bid_price: 4.95,
            ask_price: 5.05,
            bid_size: 100.0,
            ask_size: 100.0,
            syn_spot: 150.0,
        };
        ctrl.process_cob_quote(&q1);

        // Large sell-side trade: mid drops significantly
        let q2 = CobSpreadQuote {
            theoretical_price: 4.5,
            bid_price: 4.45,
            ask_price: 4.55,
            bid_size: 200.0,
            ask_size: 50.0,
            syn_spot: 149.0,
        };
        let skew = ctrl.process_cob_quote(&q2);
        assert!(skew.legging_risk_detected, "Large sell should trigger skew");
        assert!(skew.bid_offset < 0.0, "Bid should be lowered on sell pressure");
    }
}