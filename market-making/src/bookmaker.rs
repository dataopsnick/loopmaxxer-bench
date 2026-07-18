//! Bookmaker: Reservation Price & Indifference Spread Quoting (Spec §18, §29)
//!
//! Combines SOFR-biased Avellaneda-Stoikov reservation pricing with
//! OFI microstructure drift and optimal indifference spread computation.

use crate::ofi::MicrostructureOFI;
use crate::risk_gate::PreTradeRiskGate;
use crate::sofr::SOFRHedgeController;
use crate::symbology::PackedAssetKey;

/// A quote produced by the bookmaking engine.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BookQuote {
    pub asset_key: u128,
    pub bid_price: f64,
    pub ask_price: f64,
    pub reservation_price: f64,
    pub spread_width: f64,
    pub ofi_drift: f64,
    pub timestamp_ns: u64,
}

/// Configuration for the bookmaking engine.
#[derive(Debug, Clone)]
pub struct BookmakerConfig {
    pub risk_aversion_gamma: f64,
    pub sofr_base_rate: f64,
    pub margin_haircut: f64,
    pub borrow_premium: f64,
    pub liquidity_kappa: f64,
    pub ofi_decay: f64,
    pub ofi_multiplier: f64,
    pub time_to_horizon: f64,
    pub max_order_qty: u32,
    pub max_price_usd: f64,
    pub max_delta: f64,
}

impl Default for BookmakerConfig {
    fn default() -> Self {
        Self {
            risk_aversion_gamma: 0.015,
            sofr_base_rate: 0.0535,
            margin_haircut: 0.15,
            borrow_premium: 0.0025,
            liquidity_kappa: 2.1,
            ofi_decay: 0.95,
            ofi_multiplier: 0.001,
            time_to_horizon: 0.45,
            max_order_qty: 1000,
            max_price_usd: 5000.0,
            max_delta: 5000.0,
        }
    }
}

/// The core bookmaking engine that produces bid/ask quotes.
pub struct Bookmaker {
    config: BookmakerConfig,
    sofr_controller: SOFRHedgeController,
    ofi: MicrostructureOFI,
    risk_gate: PreTradeRiskGate,
}

impl Bookmaker {
    pub fn new(config: BookmakerConfig) -> Self {
        let risk_gate = PreTradeRiskGate::new(
            config.max_order_qty,
            config.max_price_usd,
            config.max_delta,
        );
        Self {
            sofr_controller: SOFRHedgeController::new(
                config.risk_aversion_gamma,
                config.sofr_base_rate,
            ),
            ofi: MicrostructureOFI::new(config.ofi_decay, config.ofi_multiplier),
            risk_gate,
            config,
        }
    }

    /// Compute the optimal bid/ask quote given current market state.
    ///
    /// Returns `None` if the pre-trade risk gate rejects the quote.
    pub fn compute_quote(
        &mut self,
        asset_key: PackedAssetKey,
        mid_price: f64,
        bid_px: f64,
        bid_sz: f64,
        ask_px: f64,
        ask_sz: f64,
        position: f64,
        volatility: f64,
        timestamp_ns: u64,
    ) -> Option<BookQuote> {
        // 1. OFI drift adjustment
        let ofi_drift = self.ofi.compute_drift_adjustment(bid_px, bid_sz, ask_px, ask_sz);

        // 2. SOFR-biased reservation price
        let reservation = self.sofr_controller.reservation_price(
            mid_price,
            position,
            volatility,
            mid_price,
            self.config.time_to_horizon,
            self.config.margin_haircut,
            self.config.borrow_premium,
        );

        // 3. Drift-corrected reservation price
        let reservation_adjusted = reservation + ofi_drift;

        // 4. Optimal indifference spread
        let spread_width = (2.0 / self.config.risk_aversion_gamma)
            * (1.0 + (self.config.risk_aversion_gamma / self.config.liquidity_kappa)).ln();

        let bid_quote = reservation_adjusted - (spread_width / 2.0);
        let ask_quote = reservation_adjusted + (spread_width / 2.0);

        // 5. Pre-trade risk gate validation
        if !self.risk_gate.validate_order(bid_quote, 100, position) {
            return None;
        }
        if !self.risk_gate.validate_order(ask_quote, 100, position) {
            return None;
        }

        Some(BookQuote {
            asset_key: asset_key.data,
            bid_price: bid_quote,
            ask_price: ask_quote,
            reservation_price: reservation_adjusted,
            spread_width,
            ofi_drift,
            timestamp_ns,
        })
    }

    /// Get a reference to the risk gate.
    pub fn risk_gate(&self) -> &PreTradeRiskGate {
        &self.risk_gate
    }

    /// Get a reference to the OFI estimator.
    pub fn ofi(&self) -> &MicrostructureOFI {
        &self.ofi
    }

    /// Get the config.
    pub fn config(&self) -> &BookmakerConfig {
        &self.config
    }
}

/// Compute the optimal indifference spread width given gamma and kappa.
///
/// δ_bid + δ_ask = (2/γ) * ln(1 + γ/κ)
#[inline(always)]
pub fn optimal_spread_width(gamma: f64, kappa: f64) -> f64 {
    (2.0 / gamma) * (1.0 + (gamma / kappa)).ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbology::sources;

    #[test]
    fn quote_generation_neutral_position() {
        let config = BookmakerConfig::default();
        let mut bm = Bookmaker::new(config);
        let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");

        let quote = bm.compute_quote(key, 150.0, 149.98, 500.0, 150.02, 500.0, 0.0, 0.20, 1000);
        assert!(quote.is_some(), "Should produce quote for neutral position");
        let q = quote.unwrap();
        assert!(q.bid_price < q.ask_price, "Bid must be below ask");
        assert!(
            (q.reservation_price - 150.0).abs() < 5.0,
            "Reservation near mid for flat position"
        );
    }

    #[test]
    fn quote_skew_on_long_position() {
        let config = BookmakerConfig::default();
        let mut bm = Bookmaker::new(config);
        let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");

        let q_flat = bm
            .compute_quote(key, 150.0, 149.98, 500.0, 150.02, 500.0, 0.0, 0.20, 1000)
            .unwrap();
        let q_long = bm
            .compute_quote(key, 150.0, 149.98, 500.0, 150.02, 500.0, 5000.0, 0.20, 1000)
            .unwrap();

        assert!(
            q_long.reservation_price < q_flat.reservation_price,
            "Long position should lower reservation price"
        );
    }

    #[test]
    fn optimal_spread_formula() {
        let spread = optimal_spread_width(0.015, 2.1);
        assert!(spread > 0.0, "Spread must be positive: {}", spread);
    }
}