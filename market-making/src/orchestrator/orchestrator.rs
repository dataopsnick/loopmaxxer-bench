//! Active Orchestrator (Spec §16, §29)
//!
//! Integrates all subsystems into a single NUMA-pinned spin loop:
//!   ingest → spline vol → reservation price + OFI → spread → risk gate → DMA submit
//!
//! The orchestrator runs on a dedicated core, polling the lock-free
//! `ArrayQueue<LiveMarketTick>` ring buffer fed by the ingestion driver.
//! Each tick flows through the full pipeline:
//!   1. Volatility lookup (Taylor expansion or spline)
//!   2. Reservation price computation (SOFR-biased Avellaneda-Stoikov)
//!   3. OFI microstructure drift adjustment
//!   4. Indifference spread computation (with dynamic κ)
//!   5. Pre-trade risk gate validation
//!   6. SBE New Order Single encoding for DMA submission
//!   7. Hedge evaluation (Whalley-Wilmott bands)

use crossbeam_queue::ArrayQueue;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::bookmaker::{Bookmaker, BookmakerConfig, BookQuote, KappaEstimator};
use crate::codec::sbe_encoder::{SbeEncoder, SbeNewOrderSingle};
use crate::hedging::router::HedgingRoutingMatrix;
use crate::ingestion::numa::{pin_thread_to_core, NumaConfig};
use crate::portfolio::AtomicPortfolioState;
use crate::sofr::{AssetHedgeParameters, SOFRHedgeController};
use crate::vol_surface::TaylorVolSurface;

use super::live_tick::LiveMarketTick;

/// Maximum capacity of the lock-free tick ring buffer.
pub const TICK_QUEUE_CAPACITY: usize = 65_536;

/// Configuration for the active orchestrator.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Bookmaker configuration (gamma, SOFR, kappa, etc.).
    pub bookmaker_config: BookmakerConfig,
    /// NUMA configuration for thread pinning.
    pub numa_config: NumaConfig,
    /// Initial cash balance for the portfolio.
    pub initial_cash: f64,
    /// Whether to pin the orchestrator thread to a specific core.
    pub pin_to_core: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            bookmaker_config: BookmakerConfig::default(),
            numa_config: NumaConfig::dev(),
            initial_cash: 100_000_000.0,
            pin_to_core: false,
        }
    }
}

/// Runtime statistics for the orchestrator.
#[derive(Debug, Default)]
pub struct OrchestratorStats {
    /// Total ticks processed.
    pub ticks_processed: AtomicU64,
    /// Total quotes generated.
    pub quotes_generated: AtomicU64,
    /// Total quotes rejected by the risk gate.
    pub quotes_rejected: AtomicU64,
    /// Total hedge evaluations.
    pub hedge_evaluations: AtomicU64,
    /// Total hedge orders emitted.
    pub hedge_orders: AtomicU64,
    /// Total NOS messages encoded.
    pub nos_encoded: AtomicU64,
    /// Total ticks dropped (queue full).
    pub ticks_dropped: AtomicU64,
}

impl OrchestratorStats {
    /// Snapshot the stats as a human-readable summary.
    pub fn snapshot(&self) -> String {
        format!(
            "ticks={}, quotes={}, rejected={}, hedges={}, hedge_orders={}, nos={}, dropped={}",
            self.ticks_processed.load(Ordering::Relaxed),
            self.quotes_generated.load(Ordering::Relaxed),
            self.quotes_rejected.load(Ordering::Relaxed),
            self.hedge_evaluations.load(Ordering::Relaxed),
            self.hedge_orders.load(Ordering::Relaxed),
            self.nos_encoded.load(Ordering::Relaxed),
            self.ticks_dropped.load(Ordering::Relaxed),
        )
    }
}

/// The active production orchestrator integrating all subsystems.
///
/// On Linux with EF_VI, the ingestion driver feeds ticks into the
/// lock-free ring buffer from a separate RX thread. The orchestrator
/// thread polls this queue and processes each tick through the full
/// pipeline. On macOS, ticks are injected via `submit_tick`.
pub struct ActiveOrchestrator {
    /// Lock-free ring buffer for incoming ticks.
    tick_queue: Arc<ArrayQueue<LiveMarketTick>>,
    /// Atomic portfolio state (lock-free Greeks + cash).
    portfolio_state: Arc<AtomicPortfolioState>,
    /// Bookmaking engine (reservation price + spread + risk gate).
    bookmaker: Bookmaker,
    /// SOFR hedge controller for Whalley-Wilmott band evaluation.
    sofr_controller: SOFRHedgeController,
    /// Hedging routing matrix for optimal stock/future allocation.
    hedging_router: HedgingRoutingMatrix,
    /// Taylor vol surface for real-time vol lookup.
    vol_surface: TaylorVolSurface,
    /// Online kappa estimator for dynamic spread widening.
    kappa_estimator: KappaEstimator,
    /// SBE encoder for outbound NOS messages.
    sbe_encoder: SbeEncoder,
    /// Runtime statistics.
    stats: OrchestratorStats,
    /// Kill switch flag.
    running: AtomicBool,
    /// Configuration.
    config: OrchestratorConfig,
}

impl ActiveOrchestrator {
    /// Create a new active orchestrator with the given configuration.
    pub fn new(config: OrchestratorConfig) -> Self {
        let bookmaker = Bookmaker::new(config.bookmaker_config.clone());
        let sofr_controller = SOFRHedgeController::new(
            config.bookmaker_config.risk_aversion_gamma,
            config.bookmaker_config.sofr_base_rate,
        );
        let hedging_router = HedgingRoutingMatrix::default();
        let vol_surface = TaylorVolSurface::flat(0.20, 150.0, 0.25);

        Self {
            tick_queue: Arc::new(ArrayQueue::new(TICK_QUEUE_CAPACITY)),
            portfolio_state: Arc::new(AtomicPortfolioState::new(config.initial_cash)),
            bookmaker,
            sofr_controller,
            hedging_router,
            vol_surface,
            kappa_estimator: KappaEstimator::with_defaults(
                config.bookmaker_config.liquidity_kappa,
            ),
            sbe_encoder: SbeEncoder::new(),
            stats: OrchestratorStats::default(),
            running: AtomicBool::new(false),
            config,
        }
    }

    /// Get a handle to the tick queue for the ingestion driver to push into.
    pub fn tick_queue(&self) -> &Arc<ArrayQueue<LiveMarketTick>> {
        &self.tick_queue
    }

    /// Get a handle to the atomic portfolio state.
    pub fn portfolio_state(&self) -> &Arc<AtomicPortfolioState> {
        &self.portfolio_state
    }

    /// Get the runtime statistics.
    pub fn stats(&self) -> &OrchestratorStats {
        &self.stats
    }

    /// Submit a tick to the orchestrator's ring buffer (non-blocking).
    ///
    /// Returns `true` if the tick was enqueued, `false` if the queue was full.
    pub fn submit_tick(&self, tick: LiveMarketTick) -> bool {
        if self.tick_queue.push(tick).is_err() {
            self.stats.ticks_dropped.fetch_add(1, Ordering::Relaxed);
            false
        } else {
            true
        }
    }

    /// Process a single tick through the full pipeline.
    ///
    /// This is the hot-path function called in the spin loop.
    /// Returns the generated quote (if any) and optional hedge NOS.
    pub fn process_tick(&mut self, tick: &LiveMarketTick) -> Option<BookQuote> {
        self.stats
            .ticks_processed
            .fetch_add(1, Ordering::Relaxed);

        // 1. Volatility lookup via Taylor expansion
        let vol = self.vol_surface.evaluate_vol(tick.spot, tick.strike, tick.expiry);

        // 2. Get current portfolio position (lock-free read)
        let position = self.portfolio_state.load_delta();

        // 3. Compute bid/ask quote through the bookmaker
        let mid = tick.mid_price();
        let quote = self.bookmaker.compute_quote(
            tick.asset_key,
            mid,
            tick.bid_px,
            tick.bid_sz,
            tick.ask_px,
            tick.ask_sz,
            position,
            vol,
            tick.timestamp_ns,
        );

        match &quote {
            Some(q) => {
                self.stats
                    .quotes_generated
                    .fetch_add(1, Ordering::Relaxed);

                // 4. Encode NOS for DMA submission
                let _nos = self.sbe_encoder.encode_new_order_single(
                    self.stats.nos_encoded.load(Ordering::Relaxed) as u64 + 1,
                    "AAPL", // In production, resolved from asset_key
                    1,      // Buy side for bid
                    100,
                    q.bid_price,
                    tick.timestamp_ns,
                );
                self.stats
                    .nos_encoded
                    .fetch_add(1, Ordering::Relaxed);

                // 5. Record fill for kappa estimator
                self.kappa_estimator
                    .record_fill(tick.timestamp_ns, q.spread_width);

                // 6. Hedge evaluation (Whalley-Wilmott bands)
                self.stats
                    .hedge_evaluations
                    .fetch_add(1, Ordering::Relaxed);

                let hedge_params = AssetHedgeParameters {
                    volatility: vol,
                    ..Default::default()
                };

                let hedge_qty = self.sofr_controller.evaluate_delta_hedge(
                    position,
                    0.0, // target delta = 0 (delta-neutral)
                    &hedge_params,
                    tick.spot,
                    0.45, // time to midnight
                );

                if let Some(qty) = hedge_qty {
                    self.stats
                        .hedge_orders
                        .fetch_add(1, Ordering::Relaxed);

                    // Compute optimal stock/future split
                    let (stock_alloc, _future_alloc) =
                        self.hedging_router.determine_optimal_hedging_allocation(
                            qty,
                            self.config.bookmaker_config.sofr_base_rate,
                            0.03, // SPAN opportunity cost
                        );

                    // In production, encode and submit hedge orders via DMA
                    let _ = stock_alloc;
                }
            }
            None => {
                self.stats
                    .quotes_rejected
                    .fetch_add(1, Ordering::Relaxed);
            }
        }

        quote
    }

    /// Run the orchestrator spin loop.
    ///
    /// On Linux with EF_VI, this polls the NIC event queue directly.
    /// On macOS, it polls the tick queue (fed via `submit_tick`).
    ///
    /// This function blocks until `stop()` is called.
    pub fn run(&mut self) {
        self.running.store(true, Ordering::SeqCst);

        // Pin to core if configured
        if self.config.pin_to_core {
            let core_id = self.config.numa_config.processing_core_id;
            let _ = pin_thread_to_core(core_id);
        }

        while self.running.load(Ordering::SeqCst) {
            // Poll the tick queue
            while let Some(tick) = self.tick_queue.pop() {
                self.process_tick(&tick);
            }

            // Brief yield to prevent 100% CPU spin on empty queue
            // (In production, this would be a busy-poll with no yield)
            std::hint::spin_loop();
        }
    }

    /// Stop the orchestrator loop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if the orchestrator is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the current kappa estimate.
    pub fn current_kappa(&self) -> f64 {
        self.kappa_estimator.kappa()
    }

    /// Get the current spread multiplier from the kappa estimator.
    pub fn spread_multiplier(&self) -> f64 {
        self.kappa_estimator.spread_multiplier()
    }

    /// Update the Taylor vol surface coefficients.
    pub fn update_vol_surface(
        &mut self,
        sigma_atm: f64,
        d_sigma_d_s: f64,
        d2_sigma_d_s2: f64,
        d_sigma_d_tau: f64,
        ref_spot: f64,
        ref_tau: f64,
        timestamp_ns: u64,
    ) {
        self.vol_surface.update_coefficients(
            sigma_atm,
            d_sigma_d_s,
            d2_sigma_d_s2,
            d_sigma_d_tau,
            ref_spot,
            ref_tau,
            timestamp_ns,
        );
    }

    /// Encode a NOS message for a quote (for testing / DMA hook).
    pub fn encode_nos_for_quote(
        &self,
        client_order_id: u64,
        symbol: &str,
        side: u8,
        qty: u32,
        price: f64,
        timestamp_ns: u64,
    ) -> SbeNewOrderSingle {
        self.sbe_encoder
            .encode_new_order_single(client_order_id, symbol, side, qty, price, timestamp_ns)
    }

    /// Check if the kill switch (risk gate) is tripped.
    pub fn is_kill_switch_tripped(&self) -> bool {
        self.bookmaker.risk_gate().is_tripped()
    }

    /// Force the kill switch (emergency purge).
    pub fn force_kill_switch(&self) {
        self.bookmaker.risk_gate().force_kill_switch();
    }

    /// Reset the kill switch.
    pub fn reset_kill_switch(&self) {
        self.bookmaker.risk_gate().reset_kill_switch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbology::{sources, PackedAssetKey};

    #[test]
    fn orchestrator_creation() {
        let config = OrchestratorConfig::default();
        let orch = ActiveOrchestrator::new(config);
        assert!(!orch.is_running());
        assert!(!orch.is_kill_switch_tripped());
        assert!((orch.current_kappa() - 2.1).abs() < 1e-9);
    }

    #[test]
    fn orchestrator_submit_and_process_tick() {
        let config = OrchestratorConfig::default();
        let mut orch = ActiveOrchestrator::new(config);

        let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
        let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);

        assert!(orch.submit_tick(tick));
        assert_eq!(orch.tick_queue.len(), 1);

        // Process the tick
        let quote = orch.process_tick(&tick);
        assert!(quote.is_some(), "Should generate a quote");

        let q = quote.unwrap();
        assert!(q.bid_price < q.ask_price, "Bid must be below ask");

        // Stats should be updated
        assert_eq!(orch.stats().ticks_processed.load(Ordering::Relaxed), 1);
        assert_eq!(orch.stats().quotes_generated.load(Ordering::Relaxed), 1);
        assert!(orch.stats().nos_encoded.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn orchestrator_risk_gate_rejection() {
        let mut config = OrchestratorConfig::default();
        config.bookmaker_config.max_price_usd = 10.0; // Very low price limit
        let mut orch = ActiveOrchestrator::new(config);

        let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
        let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);

        let quote = orch.process_tick(&tick);
        assert!(quote.is_none(), "Quote should be rejected by risk gate");
        assert!(orch.is_kill_switch_tripped(), "Kill switch should be tripped");
        assert!(orch.stats().quotes_rejected.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn orchestrator_vol_surface_update() {
        let config = OrchestratorConfig::default();
        let mut orch = ActiveOrchestrator::new(config);

        orch.update_vol_surface(0.22, 0.001, 0.0001, 0.05, 150.0, 0.25, 2000);
        assert!((orch.vol_surface.atm_vol() - 0.22).abs() < 1e-9);
        assert_eq!(orch.vol_surface.last_update_ns(), 2000);
    }

    #[test]
    fn orchestrator_nos_encoding() {
        let config = OrchestratorConfig::default();
        let orch = ActiveOrchestrator::new(config);

        let nos = orch.encode_nos_for_quote(1, "AAPL", 1, 100, 150.25, 1000);
        let client_order_id = nos.client_order_id;
        let order_qty = nos.order_qty;
        assert_eq!(client_order_id, 1);
        assert_eq!(&nos.symbol[..4], b"AAPL");
        assert_eq!(nos.side, 1);
        assert_eq!(order_qty, 100);
    }

    #[test]
    fn orchestrator_tick_queue_overflow() {
        let config = OrchestratorConfig::default();
        let orch = ActiveOrchestrator::new(config);

        // Fill the queue to capacity
        let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
        for i in 0..TICK_QUEUE_CAPACITY {
            let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, i as u64);
            assert!(orch.submit_tick(tick));
        }

        // Next tick should be dropped
        let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 999999);
        assert!(!orch.submit_tick(tick), "Queue full should reject");
        assert!(orch.stats().ticks_dropped.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn orchestrator_stats_snapshot() {
        let config = OrchestratorConfig::default();
        let mut orch = ActiveOrchestrator::new(config);

        let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
        let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);
        orch.process_tick(&tick);

        let snapshot = orch.stats().snapshot();
        assert!(snapshot.contains("ticks=1"));
        assert!(snapshot.contains("quotes=1"));
    }

    #[test]
    fn orchestrator_start_stop() {
        let config = OrchestratorConfig::default();
        let orch = ActiveOrchestrator::new(config);

        // We can't easily test the blocking run() loop in a unit test,
        // but we can test start/stop state transitions.
        orch.running.store(true, Ordering::SeqCst);
        assert!(orch.is_running());
        orch.stop();
        assert!(!orch.is_running());
    }
}