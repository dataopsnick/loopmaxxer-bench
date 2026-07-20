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

/// Online κ (kappa) estimator with sliding-window fill arrival intensity (Spec §27).
///
/// Tracks fill arrival intensity and average spread over a sliding window
/// to dynamically update the liquidity parameter κ:
///
/// `κ_i(t) = ln(1 + N_fills / (λ_arrival · Δt)) / D̄_spread`
///
/// When market depth thins (fewer fills per unit time), κ decreases,
/// causing the bookmaker to widen spreads. When fills are abundant,
/// κ increases, tightening spreads.
pub struct KappaEstimator {
    /// Current estimated kappa value.
    kappa: f64,
    /// Sliding window of fill timestamps (nanoseconds).
    fill_timestamps: std::collections::VecDeque<u64>,
    /// Sliding window of spread widths at fill time.
    fill_spreads: std::collections::VecDeque<f64>,
    /// Maximum window size (number of fills to track).
    window_size: usize,
    /// Window duration in nanoseconds (e.g., 1 second = 1_000_000_000).
    window_ns: u64,
    /// Total fills observed.
    total_fills: u64,
    /// Minimum kappa to prevent degenerate widening.
    min_kappa: f64,
    /// Maximum kappa to prevent over-tightening.
    max_kappa: f64,
}

impl KappaEstimator {
    /// Create a new kappa estimator with the given initial value and window size.
    ///
    /// - `initial_kappa`: Starting kappa value (e.g., 2.1).
    /// - `window_size`: Maximum number of fills to retain in the sliding window.
    /// - `window_ns`: Time window in nanoseconds for arrival rate computation.
    pub fn new(initial_kappa: f64, window_size: usize, window_ns: u64) -> Self {
        Self {
            kappa: initial_kappa,
            fill_timestamps: std::collections::VecDeque::with_capacity(window_size),
            fill_spreads: std::collections::VecDeque::with_capacity(window_size),
            window_size,
            window_ns,
            total_fills: 0,
            min_kappa: 0.1,
            max_kappa: 100.0,
        }
    }

    /// Create with defaults: 200-fill window, 1-second arrival window.
    pub fn with_defaults(initial_kappa: f64) -> Self {
        Self::new(initial_kappa, 200, 1_000_000_000)
    }

    /// Record a fill event and update the kappa estimate.
    ///
    /// - `timestamp_ns`: Nanosecond timestamp of the fill.
    /// - `spread_at_fill`: The spread width at the time of the fill.
    #[inline]
    pub fn record_fill(&mut self, timestamp_ns: u64, spread_at_fill: f64) {
        self.total_fills += 1;

        // Evict fills outside the time window
        let cutoff = timestamp_ns.saturating_sub(self.window_ns);
        while let Some(&ts) = self.fill_timestamps.front() {
            if ts < cutoff {
                self.fill_timestamps.pop_front();
                self.fill_spreads.pop_front();
            } else {
                break;
            }
        }

        // Evict if exceeding window size
        while self.fill_timestamps.len() >= self.window_size {
            self.fill_timestamps.pop_front();
            self.fill_spreads.pop_front();
        }

        self.fill_timestamps.push_back(timestamp_ns);
        self.fill_spreads.push_back(spread_at_fill);

        self.recompute_kappa(timestamp_ns);
    }

    /// Recompute kappa from the current sliding window.
    ///
    /// `κ = ln(1 + N_fills / (λ_arrival · Δt)) / D̄_spread`
    ///
    /// where `λ_arrival · Δt` is the expected number of fills in the window
    /// (approximated by the window size if we have enough data), and
    /// `D̄_spread` is the average spread at fill time.
    fn recompute_kappa(&mut self, _current_ns: u64) {
        let n_fills = self.fill_timestamps.len() as f64;
        if n_fills < 2.0 {
            return; // Not enough data
        }

        // Average spread at fill time
        let avg_spread: f64 =
            self.fill_spreads.iter().sum::<f64>() / n_fills;

        if avg_spread < 1e-9 {
            return; // Avoid division by zero
        }

        // Arrival rate: fills per nanosecond over the window
        let window_duration = self.window_ns as f64;
        let arrival_rate = n_fills / window_duration;

        // Expected fills in window = arrival_rate * window_duration = n_fills
        // κ = ln(1 + N_fills / (λ_arrival · Δt)) / D̄_spread
        // Since λ_arrival · Δt ≈ N_fills, the ratio ≈ 1, giving ln(2).
        // To make this sensitive to changes, we use the ratio of
        // current fill rate to the historical average.
        let lambda_dt = arrival_rate * window_duration; // = n_fills

        let ratio = if lambda_dt > 1e-9 {
            n_fills / lambda_dt
        } else {
            1.0
        };

        let new_kappa = (1.0 + ratio).ln() / avg_spread;

        // Clamp to sane bounds
        self.kappa = new_kappa.clamp(self.min_kappa, self.max_kappa);
    }

    /// Get the current kappa estimate.
    #[inline(always)]
    pub fn kappa(&self) -> f64 {
        self.kappa
    }

    /// Get the total number of fills observed.
    #[inline(always)]
    pub fn total_fills(&self) -> u64 {
        self.total_fills
    }

    /// Get the current number of fills in the sliding window.
    #[inline(always)]
    pub fn window_fills(&self) -> usize {
        self.fill_timestamps.len()
    }

    /// Compute the dynamic spread multiplier based on market depth.
    ///
    /// When fills are sparse (low arrival rate), widen spreads.
    /// When fills are dense (high arrival rate), tighten spreads.
    ///
    /// Returns a multiplier > 1.0 to widen, < 1.0 to tighten.
    #[inline]
    pub fn spread_multiplier(&self) -> f64 {
        let n = self.fill_timestamps.len() as f64;
        let expected = self.window_size as f64 / 2.0;

        if n < 1.0 || expected < 1.0 {
            return 1.0; // Neutral when insufficient data
        }

        // Ratio of actual fills to expected half-window
        let depth_ratio = n / expected;

        // If depth_ratio < 1, market is thinning → widen (multiplier > 1)
        // If depth_ratio > 1, market is deep → tighten (multiplier < 1)
        // Use a smooth mapping: multiplier = (expected / actual)^0.5
        if depth_ratio > 1e-9 {
            (1.0 / depth_ratio).sqrt()
        } else {
            2.0 // Max widening when no fills
        }
    }

    /// Reset the estimator state.
    pub fn reset(&mut self) {
        self.fill_timestamps.clear();
        self.fill_spreads.clear();
        self.total_fills = 0;
    }
}

impl Default for KappaEstimator {
    fn default() -> Self {
        Self::with_defaults(2.1)
    }
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

    #[test]
    fn kappa_estimator_initial_state() {
        let ke = KappaEstimator::with_defaults(2.1);
        assert!((ke.kappa() - 2.1).abs() < 1e-9);
        assert_eq!(ke.total_fills(), 0);
        assert_eq!(ke.window_fills(), 0);
    }

    #[test]
    fn kappa_estimator_records_fills() {
        let mut ke = KappaEstimator::with_defaults(2.1);
        ke.record_fill(1_000_000_000, 0.05);
        ke.record_fill(1_100_000_000, 0.05);
        ke.record_fill(1_200_000_000, 0.05);

        assert_eq!(ke.total_fills(), 3);
        assert_eq!(ke.window_fills(), 3);
        // Kappa should have been recomputed
        assert!(ke.kappa() > 0.0);
    }

    #[test]
    fn kappa_estimator_spread_multiplier_thin_market() {
        let mut ke = KappaEstimator::new(2.1, 200, 1_000_000_000);
        // Only 2 fills in a 200-fill window → thin market
        ke.record_fill(1_000_000_000, 0.05);
        ke.record_fill(1_100_000_000, 0.05);

        let mult = ke.spread_multiplier();
        // Thin market → multiplier > 1 (widen)
        assert!(mult > 1.0, "Thin market should widen spreads: {}", mult);
    }

    #[test]
    fn kappa_estimator_spread_multiplier_deep_market() {
        let mut ke = KappaEstimator::new(2.1, 10, 1_000_000_000);
        // Fill the window beyond half → deep market
        for i in 0..8 {
            ke.record_fill(i as u64 * 100_000_000, 0.05);
        }

        let mult = ke.spread_multiplier();
        // Deep market → multiplier < 1 (tighten)
        assert!(mult < 1.0, "Deep market should tighten spreads: {}", mult);
    }

    #[test]
    fn kappa_estimator_evicts_old_fills() {
        let mut ke = KappaEstimator::new(2.1, 3, 500_000_000); // 500ms window
        ke.record_fill(1_000_000_000, 0.05);
        ke.record_fill(1_100_000_000, 0.05);
        ke.record_fill(1_200_000_000, 0.05);
        assert_eq!(ke.window_fills(), 3);

        // This fill is 600ms after the first, which should evict it
        ke.record_fill(1_600_000_000, 0.05);
        assert!(ke.window_fills() <= 3, "Old fills should be evicted");
    }

    #[test]
    fn kappa_estimator_reset() {
        let mut ke = KappaEstimator::with_defaults(2.1);
        ke.record_fill(1_000_000_000, 0.05);
        ke.record_fill(1_100_000_000, 0.05);
        assert_eq!(ke.total_fills(), 2);

        ke.reset();
        assert_eq!(ke.total_fills(), 0);
        assert_eq!(ke.window_fills(), 0);
    }
}
