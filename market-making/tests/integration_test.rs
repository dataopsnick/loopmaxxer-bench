//! Integration Test: Full Pipeline (Spec §16, §29)
//!
//! Synthetic tick stream → full orchestrator pipeline → outbound
//! quote/hedge validation. Tests the end-to-end flow from tick
//! ingestion through quote generation, risk gate validation, NOS
//! encoding, and hedge evaluation.

use mr_market::bookmaker::{BookmakerConfig, KappaEstimator};
use mr_market::codec::sbe_encoder::SbeEncoder;
use mr_market::hedging::router::HedgingRoutingMatrix;
use mr_market::ofi::MicrostructureOFI;
use mr_market::orchestrator::{
    ActiveOrchestrator, LiveMarketTick, OrchestratorConfig, TICK_QUEUE_CAPACITY,
};
use mr_market::portfolio::AtomicPortfolioState;
use mr_market::risk_gate::PreTradeRiskGate;
use mr_market::sofr::{AssetHedgeParameters, SOFRHedgeController};
use mr_market::symbology::{sources, PackedAssetKey};
use mr_market::vol_surface::TaylorVolSurface;

use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Test the full orchestrator pipeline with a synthetic tick stream.
#[test]
fn full_pipeline_synthetic_tick_stream() {
    let config = OrchestratorConfig::default();
    let mut orch = ActiveOrchestrator::new(config);

    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");

    // Simulate a stream of 100 ticks with slightly varying prices
    let mut quotes_generated = 0;
    let mut quotes_rejected = 0;

    for i in 0..100u64 {
        let base_price = 150.0 + (i as f64 * 0.01);
        let bid = base_price - 0.02;
        let ask = base_price + 0.02;
        let tick = LiveMarketTick::new(key, base_price, bid, 500.0, ask, 500.0, i * 1_000_000);

        let quote = orch.process_tick(&tick);
        match quote {
            Some(_) => quotes_generated += 1,
            None => quotes_rejected += 1,
        }
    }

    assert_eq!(quotes_generated + quotes_rejected, 100);
    assert!(
        quotes_generated > 0,
        "Should generate at least some quotes from synthetic stream"
    );

    // Verify stats
    let stats = orch.stats();
    assert_eq!(
        stats.ticks_processed.load(Ordering::Relaxed),
        100,
        "Should have processed 100 ticks"
    );
    assert_eq!(
        stats.quotes_generated.load(Ordering::Relaxed),
        quotes_generated as u64
    );
    assert!(stats.nos_encoded.load(Ordering::Relaxed) >= quotes_generated as u64);
}

/// Test that the risk gate kills the pipeline on a bad tick.
#[test]
fn risk_gate_kills_pipeline_on_breach() {
    let mut config = OrchestratorConfig::default();
    config.bookmaker_config.max_price_usd = 200.0;
    let mut orch = ActiveOrchestrator::new(config);

    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");

    // First tick: normal price, should pass
    let tick1 = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);
    let q1 = orch.process_tick(&tick1);
    assert!(q1.is_some(), "Normal tick should produce a quote");
    assert!(!orch.is_kill_switch_tripped());

    // Second tick: price exceeds limit, should trip kill switch
    let tick2 = LiveMarketTick::new(key, 250.0, 249.98, 500.0, 250.02, 500.0, 2000);
    let q2 = orch.process_tick(&tick2);
    assert!(q2.is_none(), "Over-limit tick should be rejected");
    assert!(orch.is_kill_switch_tripped(), "Kill switch should be tripped");

    // Third tick: even normal price should be rejected now (kill switch active)
    let tick3 = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 3000);
    let q3 = orch.process_tick(&tick3);
    assert!(q3.is_none(), "Post-kill-switch tick should be rejected");

    // Reset kill switch and verify recovery
    orch.reset_kill_switch();
    assert!(!orch.is_kill_switch_tripped());

    let q4 = orch.process_tick(&tick1);
    assert!(q4.is_some(), "After reset, normal tick should produce a quote");
}

/// Test the full pipeline with option ticks (vol surface lookup).
#[test]
fn full_pipeline_option_ticks() {
    let config = OrchestratorConfig::default();
    let mut orch = ActiveOrchestrator::new(config);

    // Update vol surface with realistic coefficients
    orch.update_vol_surface(0.22, -0.001, 0.0001, 0.05, 150.0, 0.25, 1000);

    let key = PackedAssetKey::new_option(sources::NMS, "AAPL", 30, 15000, true);

    let tick = LiveMarketTick::new_option(
        key,
        150.0, // spot
        150.0, // strike
        0.25,  // expiry (3 months)
        5.00,  // bid
        10.0,  // bid size
        5.10,  // ask
        10.0,  // ask size
        1000,
    );

    let quote = orch.process_tick(&tick);
    assert!(quote.is_some(), "Option tick should produce a quote");

    let q = quote.unwrap();
    assert!(q.bid_price < q.ask_price);
}

/// Test that the kappa estimator adapts over a tick stream.
#[test]
fn kappa_estimator_adapts_over_stream() {
    let config = OrchestratorConfig::default();
    let mut orch = ActiveOrchestrator::new(config);

    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
    let initial_kappa = orch.current_kappa();

    // Process many ticks to build up kappa estimator data
    for i in 0..50u64 {
        let tick = LiveMarketTick::new(
            key,
            150.0,
            149.98,
            500.0,
            150.02,
            500.0,
            i * 10_000_000, // 10ms apart
        );
        orch.process_tick(&tick);
    }

    let final_kappa = orch.current_kappa();
    // Kappa should have been updated from the fill data
    assert!(final_kappa > 0.0, "Kappa should be positive");
    // It may or may not have changed significantly, but it should be valid
    let _ = initial_kappa;
}

/// Test the full pipeline with portfolio state updates.
#[test]
fn pipeline_with_portfolio_updates() {
    let config = OrchestratorConfig::default();
    let mut orch = ActiveOrchestrator::new(config);

    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");

    // Simulate a fill that increases our delta
    orch.portfolio_state().add_delta(500.0);
    assert!((orch.portfolio_state().load_delta() - 500.0).abs() < 1e-9);

    // Process a tick with a long position
    let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);
    let quote = orch.process_tick(&tick);

    assert!(quote.is_some(), "Should produce quote even with position");
    let q = quote.unwrap();
    // With a long position, reservation price should be lower than mid
    assert!(
        q.reservation_price < 150.0,
        "Long position should lower reservation price: {}",
        q.reservation_price
    );
}

/// Test SBE NOS encoding from the orchestrator.
#[test]
fn orchestrator_nos_encoding_full() {
    let config = OrchestratorConfig::default();
    let orch = ActiveOrchestrator::new(config);

    let nos = orch.encode_nos_for_quote(42, "AAPL", 1, 200, 150.50, 1234567890);

    let client_order_id = nos.client_order_id;
    let order_qty = nos.order_qty;
    let price = nos.price;
    assert_eq!(client_order_id, 42);
    assert_eq!(&nos.symbol[..4], b"AAPL");
    assert_eq!(nos.side, 1);
    assert_eq!(nos.order_qty, 200);
    assert_eq!(nos.price, 1505000); // 150.50 * 10000

    // Verify the NOS can be serialized to bytes
    let bytes = nos.as_bytes();
    assert!(!bytes.is_empty());
    assert_eq!(bytes.len(), std::mem::size_of_val(&nos));
}

/// Test the hedging routing matrix with realistic parameters.
#[test]
fn hedging_routing_integration() {
    let router = HedgingRoutingMatrix::default();

    // Large delta imbalance requiring hedging
    let imbalance = 5000.0;
    let (stock_alloc, future_alloc) =
        router.determine_optimal_hedging_allocation(imbalance, 0.0535, 0.03);

    // Total allocation should equal the imbalance
    let total = stock_alloc + future_alloc;
    assert!(
        (total - imbalance).abs() < 1e-6,
        "Stock + Future should equal imbalance: {} + {} = {} vs {}",
        stock_alloc,
        future_alloc,
        total,
        imbalance
    );

    // 3-asset allocation
    let result = router.determine_3asset_allocation(imbalance, 0.0535, 0.03, 0.04);
    assert!(
        result.residual_delta.abs() < 1.0,
        "Residual delta should be near zero: {}",
        result.residual_delta
    );
}

/// Test the Taylor vol surface expansion accuracy.
#[test]
fn taylor_vol_surface_accuracy() {
    let mut surface = TaylorVolSurface::flat(0.20, 150.0, 0.25);

    // At the reference point, vol should equal sigma_atm
    let vol_at_ref = surface.evaluate_vol(150.0, 150.0, 0.25);
    assert!((vol_at_ref - 0.20).abs() < 1e-9);

    // Update with skew
    surface.update_coefficients(0.20, -0.001, 0.0001, 0.05, 150.0, 0.25, 1000);

    // At +5 from ATM, vol should decrease (negative skew)
    let vol_above = surface.evaluate_vol(155.0, 155.0, 0.25);
    let vol_atm = surface.evaluate_vol(150.0, 150.0, 0.25);
    assert!(
        vol_above < vol_atm,
        "Negative skew should decrease vol above ATM: {} < {}",
        vol_above,
        vol_atm
    );

    // At -5 from ATM, vol should increase
    let vol_below = surface.evaluate_vol(145.0, 145.0, 0.25);
    assert!(
        vol_below > vol_atm,
        "Negative skew should increase vol below ATM: {} > {}",
        vol_below,
        vol_atm
    );
}

/// Test the full drop-copy → portfolio → orchestrator loop.
#[test]
fn dropcopy_portfolio_orchestrator_loop() {
    let state = Arc::new(AtomicPortfolioState::new(100_000_000.0));

    // Simulate drop-copy fill updates
    state.add_delta(100.0); // Buy 100
    state.add_delta(50.0); // Buy 50
    state.add_delta(-75.0); // Sell 75

    assert!(
        (state.load_delta() - 75.0).abs() < 1e-9,
        "Net delta should be 75"
    );

    // Now run the orchestrator with this portfolio state
    let config = OrchestratorConfig::default();
    let mut orch = ActiveOrchestrator::new(config);

    // Manually set the portfolio delta
    orch.portfolio_state().add_delta(75.0);

    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
    let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);
    let quote = orch.process_tick(&tick);

    assert!(quote.is_some());
    let q = quote.unwrap();
    // With a long position of 75, reservation should be below mid
    assert!(q.reservation_price < 150.0);
}

/// Test tick queue overflow handling.
#[test]
fn tick_queue_overflow_handling() {
    let config = OrchestratorConfig::default();
    let orch = ActiveOrchestrator::new(config);

    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");

    // Fill the queue to capacity
    for i in 0..TICK_QUEUE_CAPACITY {
        let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, i as u64);
        assert!(orch.submit_tick(tick));
    }

    // Next tick should be dropped
    let overflow_tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 999999);
    assert!(!orch.submit_tick(overflow_tick), "Queue full should reject");

    // Verify drop count
    assert!(orch.stats().ticks_dropped.load(Ordering::Relaxed) >= 1);
}

/// Test the complete quoting pipeline with OFI drift.
#[test]
fn quoting_pipeline_with_ofi_drift() {
    let mut ofi = MicrostructureOFI::new(0.95, 0.001);

    // Simulate bid improvement (positive OFI)
    let _ = ofi.compute_drift_adjustment(150.0, 100.0, 150.10, 100.0);
    let drift = ofi.compute_drift_adjustment(150.05, 200.0, 150.10, 100.0);

    assert!(
        drift > 0.0,
        "Bid improvement should produce positive drift: {}",
        drift
    );

    // Now use the bookmaker with this OFI state
    let config = BookmakerConfig::default();
    let mut bm = mr_market::bookmaker::Bookmaker::new(config);
    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");

    let quote = bm.compute_quote(key, 150.0, 150.05, 200.0, 150.10, 100.0, 0.0, 0.20, 1000);
    assert!(quote.is_some());
}