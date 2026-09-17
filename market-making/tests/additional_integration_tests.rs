//! Corrected TDD Integration and Unit Test Suite for Backlog Bugs
//! (Tasks 13, 21, 23, 26, 32, 42, 44)
//!
//! This file supersedes `tests/backlog_tdd_integration_tests.rs`.
//!
//! Six of the seven original tests (Tasks 13, 21, 26, 32, 42, 44) are
//! preserved verbatim: they are behavior-driven, cannot be satisfied by a
//! lazy/hardcoded implementation, and fixing the code they exercise is an
//! unambiguous improvement to the system. They are expected to be RED until
//! the corresponding architectural fix lands, and GREEN afterward.
//!
//! The seventh (Task 23, original) is commented out below rather than
//! deleted. Its own "expected" oracle computed a reservation price using
//! `position = 500.0` fed directly into `SOFRHedgeController::reservation_price`
//! while the *actual* position applied to the orchestrator under test was
//! left at `0.0` -- i.e. the oracle silently described a different scenario
//! than the one it was comparing against. Bending the implementation to
//! match that oracle would require quoting a ~$5.10 option near ~$94.61,
//! reintroducing the exact "prices options at the underlying's spot instead
//! of its own premium" defect that was already fixed under Task 39. See
//! `tests/FAILURE-IS-GOOD-ACTUALLY.md` for the full closed-form derivation
//! and proof. A corrected replacement,
//! `test_task_23_orchestrator_uses_tims_margin_model_corrected`, is provided
//! below: it isolates the SOFR/haircut term from the (much larger,
//! magnitude-scaling) Avellaneda-Stoikov inventory risk penalty by using a
//! minimal non-zero seeded position, so the still-real, still-unfixed gap
//! (TimsMarginModel is implemented but never wired into the hot path) can be
//! detected without smuggling in a pricing-basis error.

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use mr_market::dropcopy::RawDropCopyListener;
    use mr_market::ingestion::driver::UserspaceIngestionDriver;
    use mr_market::ingestion::spider_stream::{
        OptionBookQuoteBody, SpiderStreamHeader, StockBookQuoteBody,
    };
    use mr_market::margin::tims::{PositionGreeks, TimsMarginModel};
    use mr_market::orchestrator::{ActiveOrchestrator, LiveMarketTick, OrchestratorConfig};
    use mr_market::sofr::SOFRHedgeController;
    use mr_market::symbology::{sources, PackedAssetKey};

    // =========================================================================
    // PRESERVED: tests whose fix is an unambiguous improvement (real bugs).
    // =========================================================================

    /// Task 13 (Drop Copy Listener not spawned):
    /// Verifies that running ActiveOrchestrator / live orchestrator actually spawns the TCP
    /// RawDropCopyListener and updates AtomicPortfolioState delta upon receiving a real
    /// FIX ExecutionReport over a TCP socket.
    #[test]
    fn test_task_13_drop_copy_listener_spawned_and_updates_portfolio() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind TCP listener");
        let local_addr = listener.local_addr().expect("Failed to get local addr");

        let config = OrchestratorConfig::default();
        let orch = Arc::new(ActiveOrchestrator::new(config));
        let portfolio = orch.portfolio_state().clone();

        let drop_copy_addr = local_addr.to_string();
        let portfolio_clone = portfolio.clone();

        let _handle = thread::spawn(move || {
            if let Ok(mut drop_listener) = RawDropCopyListener::new(&drop_copy_addr, portfolio_clone) {
                drop_listener.start_listening_loop();
            }
        });

        let (mut stream, _) = listener.accept().expect("Failed to accept TCP connection");

        let fix_frame = b"8=FIX.4.4\x0135=8\x01150=2\x0154=1\x0132=100.0\x0110=000\x01";
        stream.write_all(fix_frame).expect("Failed to send FIX frame over TCP");
        stream.flush().expect("Failed to flush TCP stream");

        thread::sleep(Duration::from_millis(50));

        let delta = portfolio.load_delta();
        assert_eq!(
            delta, 100.0,
            "Task 13 Failure: Live orchestrator portfolio state delta failed to update from drop copy stream (got {}, expected 100.0)",
            delta
        );
    }

    /// Task 21 (Option ticks dropped):
    /// Verifies that `parse_spider_stream_frame` parses option ticks (`message_type = 1060`)
    /// and extracts option fields (`bid_vol`, `ask_vol`, option prices) matching the wire payload.
    #[test]
    fn test_task_21_option_ticks_parsed_from_spider_stream() {
        let header = SpiderStreamHeader {
            sys_environment: 1,
            message_type: 1060, // OptionBookQuote message type
            source_id: 100,
            sequence_number: 1,
            sent_time: 1600000000000000000,
            message_length: 48,
            key_length: 12,
        };

        let symbol_key = b"AAPL261218C0"; // 12-byte symbol key

        let option_body = OptionBookQuoteBody {
            bid_price: 5.50,
            ask_price: 5.70,
            bid_size: 10,
            ask_size: 15,
            bid_vol: 0.25,
            ask_vol: 0.27,
        };

        let header_size = SpiderStreamHeader::SIZE;
        let body_size = std::mem::size_of::<OptionBookQuoteBody>();
        let total_size = header_size + 12 + body_size;

        let mut frame = vec![0u8; total_size];

        unsafe {
            std::ptr::write_unaligned(frame.as_mut_ptr() as *mut SpiderStreamHeader, header);
            std::ptr::copy_nonoverlapping(symbol_key.as_ptr(), frame.as_mut_ptr().add(header_size), 12);
            std::ptr::write_unaligned(
                frame.as_mut_ptr().add(header_size + 12) as *mut OptionBookQuoteBody,
                option_body,
            );
        }

        let parsed = UserspaceIngestionDriver::parse_spider_stream_frame(&frame);
        assert!(
            parsed.is_some(),
            "Task 21 Failure: Option tick frame was dropped by parse_spider_stream_frame"
        );

        let tick = parsed.unwrap();
        assert!(
            tick.is_option(),
            "Task 21 Failure: IngestedTick is_option was false for message_type = 1060"
        );
        assert_eq!(
            tick.bid_vol, 0.25,
            "Task 21 Failure: Option bid_vol was not extracted (got {}, expected 0.25)",
            tick.bid_vol
        );
        assert_eq!(
            tick.ask_vol, 0.27,
            "Task 21 Failure: Option ask_vol was not extracted (got {}, expected 0.27)",
            tick.ask_vol
        );
        assert_eq!(
            tick.bid_price, 5.50,
            "Task 21 Failure: Option bid_price was not extracted (got {}, expected 5.50)",
            tick.bid_price
        );
        assert_eq!(
            tick.ask_price, 5.70,
            "Task 21 Failure: Option ask_price was not extracted (got {}, expected 5.70)",
            tick.ask_price
        );
    }

    /// Task 26 (Hardcoded AAPL):
    /// Verifies that `parse_spider_stream_frame` extracts the symbol directly from the 12-byte wire
    /// payload and encodes it into `PackedAssetKey`, rather than hardcoding "AAPL".
    #[test]
    fn test_task_26_symbol_extracted_dynamically_from_wire() {
        let header = SpiderStreamHeader {
            sys_environment: 1,
            message_type: 1050, // StockBookQuote
            source_id: 1,
            sequence_number: 1,
            sent_time: 1000,
            message_length: 36,
            key_length: 12,
        };

        let body = StockBookQuoteBody {
            bid_price: 200.0,
            ask_price: 200.5,
            bid_size: 100,
            ask_size: 100,
        };

        let mut frame_tsla = vec![0u8; SpiderStreamHeader::SIZE + 12 + StockBookQuoteBody::SIZE];
        unsafe {
            std::ptr::write_unaligned(frame_tsla.as_mut_ptr() as *mut SpiderStreamHeader, header);
            std::ptr::copy_nonoverlapping(b"TSLA        ".as_ptr(), frame_tsla.as_mut_ptr().add(SpiderStreamHeader::SIZE), 12);
            std::ptr::write_unaligned(
                frame_tsla.as_mut_ptr().add(SpiderStreamHeader::SIZE + 12) as *mut StockBookQuoteBody,
                body,
            );
        }

        let mut frame_nvda = vec![0u8; SpiderStreamHeader::SIZE + 12 + StockBookQuoteBody::SIZE];
        unsafe {
            std::ptr::write_unaligned(frame_nvda.as_mut_ptr() as *mut SpiderStreamHeader, header);
            std::ptr::copy_nonoverlapping(b"NVDA        ".as_ptr(), frame_nvda.as_mut_ptr().add(SpiderStreamHeader::SIZE), 12);
            std::ptr::write_unaligned(
                frame_nvda.as_mut_ptr().add(SpiderStreamHeader::SIZE + 12) as *mut StockBookQuoteBody,
                body,
            );
        }

        let tick_tsla = UserspaceIngestionDriver::parse_spider_stream_frame(&frame_tsla)
            .expect("Failed to parse TSLA frame");
        let tick_nvda = UserspaceIngestionDriver::parse_spider_stream_frame(&frame_nvda)
            .expect("Failed to parse NVDA frame");

        let data_tsla = tick_tsla.asset_key.data;
        let data_nvda = tick_nvda.asset_key.data;
        assert_ne!(
            data_tsla, data_nvda,
            "Task 26 Failure: Both frames produced identical asset_key.data (hardcoded AAPL bug)"
        );

        assert_eq!(
            tick_tsla.asset_key.symbol(), "TSLA",
            "Task 26 Failure: Ingestion parser hardcoded AAPL instead of extracting TSLA (got {})",
            tick_tsla.asset_key.symbol()
        );

        assert_eq!(
            tick_nvda.asset_key.symbol(), "NVDA",
            "Task 26 Failure: Ingestion parser hardcoded AAPL instead of extracting NVDA (got {})",
            tick_nvda.asset_key.symbol()
        );
    }

    /// Task 32 (Kappa estimator unused):
    /// Verifies that the bookmaker dynamically adjusts `spread_width` using the online `KappaEstimator`
    /// spread multiplier. Fills tighten spreads; absence of fills widens spreads.
    #[test]
    fn test_task_32_bookmaker_uses_dynamic_kappa_spread_multiplier() {
        let config = OrchestratorConfig::default();
        let mut orch = ActiveOrchestrator::new(config);

        let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
        let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);

        let q1 = orch.process_tick(&tick).expect("Failed to get initial quote");

        for i in 0..500 {
            orch.record_fill(1000 + i * 1_000_000, q1.spread_width);
        }

        let q2 = orch.process_tick(&tick).expect("Failed to get quote after fills");

        assert!(
            q2.spread_width < q1.spread_width,
            "Task 32 Failure: High fill density failed to tighten spread_width (q1={}, q2={})",
            q1.spread_width, q2.spread_width
        );

        let late_tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 10_000_000_000);
        let q3 = orch.process_tick(&late_tick).expect("Failed to get late quote");

        assert!(
            q3.spread_width > q2.spread_width,
            "Task 32 Failure: Lack of fills failed to widen spread_width (q2={}, q3={})",
            q2.spread_width, q3.spread_width
        );
    }

    /// Task 42 (Global delta contamination):
    /// Verifies that `AtomicPortfolioState` tracks inventory per asset, ensuring inventory
    /// in AAPL (+5000 delta) does not skew quotes for TSLA (0 inventory).
    #[test]
    fn test_task_42_per_asset_inventory_tracking_prevents_cross_asset_skew() {
        let config = OrchestratorConfig::default();
        let mut orch = ActiveOrchestrator::new(config);

        let key_aapl = PackedAssetKey::new_equity(sources::NMS, "AAPL");
        let key_tsla = PackedAssetKey::new_equity(sources::NMS, "TSLA");

        orch.portfolio_state().add_delta(5000.0);

        let tick_tsla = LiveMarketTick::new(key_tsla, 200.0, 199.98, 100.0, 200.02, 100.0, 1000);
        let q_tsla = orch.process_tick(&tick_tsla).expect("Failed to produce TSLA quote");

        let tsla_skew = (q_tsla.reservation_price - 200.0).abs();
        assert!(
            tsla_skew < 0.05,
            "Task 42 Failure: TSLA quote was skewed by AAPL inventory (reservation_price={}, mid=200.0, skew={})",
            q_tsla.reservation_price, tsla_skew
        );

        let tick_aapl = LiveMarketTick::new(key_aapl, 150.0, 149.98, 100.0, 150.02, 100.0, 2000);
        let q_aapl = orch.process_tick(&tick_aapl).expect("Failed to produce AAPL quote");

        assert!(
            q_aapl.reservation_price < 149.0,
            "Task 42 Failure: AAPL quote was not skewed downward for +5000 AAPL inventory (reservation_price={})",
            q_aapl.reservation_price
        );
    }

    /// Task 44 (DMA Memory Corruption / Teardown):
    /// Verifies that dropping `UserspaceIngestionDriver` invokes proper C FFI teardown
    /// (`ef_memreg_free`, `ef_vi_free`), releasing all allocated DMA buffer slots.
    #[test]
    fn test_task_44_dma_driver_teardown_frees_allocated_slots() {
        let driver = UserspaceIngestionDriver::new_dev();
        let allocated_slots = driver.allocated_dma_slots();

        assert!(
            allocated_slots > 0,
            "Precondition: UserspaceIngestionDriver should allocate DMA slots"
        );

        let freed_before = driver.freed_dma_slots();
        drop(driver);

        assert_eq!(
            freed_before, allocated_slots,
            "Task 44 Failure: UserspaceIngestionDriver dropped without freeing DMA memory slots (allocated={}, freed={})",
            allocated_slots, freed_before
        );
    }

    // =========================================================================
    // COMMENTED OUT: original Task 23 test. Its oracle is self-contradictory
    // (asserts about position=500.0 but never applies that position to the
    // orchestrator under test), and "fixing" the implementation to satisfy it
    // would degrade correctness by reviving the Task 39 premium/spot bug.
    // See tests/FAILURE-IS-GOOD-ACTUALLY.md for the full proof.
    // =========================================================================
    //
    // #[test]
    // fn test_task_23_orchestrator_uses_tims_margin_model() {
    //     let mut config = OrchestratorConfig::default();
    //     config.bookmaker_config.risk_aversion_gamma = 0.015;
    //     config.bookmaker_config.sofr_base_rate = 0.0535;
    //
    //     let mut orch = ActiveOrchestrator::new(config);
    //
    //     let call_pos = PositionGreeks {
    //         spot: 100.0, delta: 0.50, gamma: 0.02, vega: 10.0, theta: -0.50, notional: 100_000.0,
    //     };
    //     let stock_pos = PositionGreeks {
    //         spot: 100.0, delta: -0.50, gamma: 0.0, vega: 0.0, theta: 0.0, notional: 50_000.0,
    //     };
    //
    //     let tims = TimsMarginModel::new(0.15, 0.0535);
    //     let tims_result = tims.evaluate_portfolio(&[call_pos, stock_pos]);
    //
    //     let static_haircut = 0.15;
    //     let tims_dynamic_haircut = (tims_result.margin_requirement / 100_000.0).clamp(0.01, 0.50);
    //
    //     let sofr_controller = SOFRHedgeController::new(0.015, 0.0535);
    //
    //     // BUG: position=500.0 is asserted about here, but is never applied
    //     // to `orch.portfolio_state()` below -- the oracle describes a
    //     // scenario the test never constructs.
    //     let expected_tims_reservation = sofr_controller.reservation_price(
    //         100.0, 500.0, 0.20, 100.0, 0.45, tims_dynamic_haircut, 0.0025,
    //     );
    //     let static_15_reservation = sofr_controller.reservation_price(
    //         100.0, 500.0, 0.20, 100.0, 0.45, static_haircut, 0.0025,
    //     );
    //
    //     // BUG: mid_price argument (100.0) is the *underlying spot*, not the
    //     // option's own premium (~5.10) -- exactly the defect fixed by Task 39.
    //     let key = PackedAssetKey::new_option(sources::NMS, "AAPL", 30, 10000, true);
    //     let tick = LiveMarketTick::new_option(key, 100.0, 100.0, 0.25, 5.0, 10.0, 5.2, 10.0, 1000);
    //
    //     let quote = orch.process_tick(&tick).expect("Failed to produce quote");
    //
    //     let reservation_diff = (quote.reservation_price - static_15_reservation).abs();
    //     assert!(reservation_diff > 0.05, "...");
    //
    //     let tims_diff = (quote.reservation_price - expected_tims_reservation).abs();
    //     assert!(tims_diff < 0.01, "...");
    // }

    // =========================================================================
    // CORRECTED REPLACEMENT for Task 23.
    // =========================================================================

    /// Task 23 (TIMS Margin unused) -- CORRECTED ORACLE.
    ///
    /// Verifies that `ActiveOrchestrator` derives its margin haircut from
    /// `TimsMarginModel`'s 17-scenario cross-asset stress grid rather than the
    /// static `0.15` default in `BookmakerConfig`.
    ///
    /// Unlike the original test, this version:
    ///   1. Seeds the exact position (`+1.0`) it reasons about on the
    ///      orchestrator actually under test (no silent scenario mismatch).
    ///   2. Uses a minimal non-zero position so the SOFR/haircut term --
    ///      `sign(q) * (SOFR + haircut + borrow) * T`, independent of `|q|` --
    ///      is cleanly isolated from the Avellaneda-Stoikov inventory risk
    ///      penalty, which scales with `q` and would otherwise swamp the
    ///      haircut signal at large position sizes.
    ///   3. Anchors its Task-39 regression guard to the option's own premium
    ///      (mid ~= 5.10), not the underlying spot (100.0).
    ///   4. First proves the TIMS haircut is numerically distinguishable from
    ///      the static 15% baseline, then asserts the live quote reflects the
    ///      TIMS-derived value rather than the static one.
    ///
    /// Expected RED until `TimsMarginModel` is wired into
    /// `ActiveOrchestrator`/`Bookmaker`'s hot path; GREEN once it is.
    #[test]
    fn test_task_23_orchestrator_uses_tims_margin_model_corrected() {
        let mut config = OrchestratorConfig::default();
        config.bookmaker_config.risk_aversion_gamma = 0.015;
        config.bookmaker_config.sofr_base_rate = 0.0535;

        let mut orch = ActiveOrchestrator::new(config);

        // Seed the exact position this test reasons about.
        orch.portfolio_state().add_delta(1.0);

        // Cross-asset TIMS scenario: long call + short stock, driving a
        // non-trivial cross-asset margin netting benefit.
        let call_pos = PositionGreeks {
            spot: 100.0,
            delta: 0.50,
            gamma: 0.02,
            vega: 10.0,
            theta: -0.50,
            notional: 100_000.0,
        };
        let stock_pos = PositionGreeks {
            spot: 100.0,
            delta: -0.50,
            gamma: 0.0,
            vega: 0.0,
            theta: 0.0,
            notional: 50_000.0,
        };
        let tims = TimsMarginModel::new(0.15, 0.0535);
        let tims_result = tims.evaluate_portfolio(&[call_pos, stock_pos]);
        let tims_dynamic_haircut = (tims_result.margin_requirement / 100_000.0).clamp(0.01, 0.50);

        let sofr_controller = SOFRHedgeController::new(0.015, 0.0535);

        // Submit the option tick: spot=underlying=100.0, mid=option premium=5.10.
        let key = PackedAssetKey::new_option(sources::NMS, "AAPL", 30, 10000, true);
        let tick = LiveMarketTick::new_option(key, 100.0, 100.0, 0.25, 5.0, 10.0, 5.2, 10.0, 1000);
        let quote = orch.process_tick(&tick).expect("Failed to produce quote");

        // Regression guard (Task 39): reservation must stay anchored near the
        // option's own premium (5.10), never drift toward the underlying spot
        // (100.0). Bound is derived from the max carry-cost drift a correct
        // model can produce at this tiny position size, plus slack.
        let max_correct_drift = (0.0535 + 0.50 + 0.0025) * 0.45 + 0.10;
        assert!(
            (quote.reservation_price - 5.10).abs() < max_correct_drift,
            "Task 39 regression: reservation_price ({}) drifted toward the underlying spot \
             (100.0) instead of staying anchored to the option premium (5.10)",
            quote.reservation_price
        );

        // Baseline: static 15% haircut on the same (correct) mid_price basis.
        let static_15_reservation =
            sofr_controller.reservation_price(5.10, 1.0, 0.20, 100.0, 0.45, 0.15, 0.0025);
        // What TIMS's dynamically-derived haircut should produce on the same basis.
        let tims_reservation = sofr_controller.reservation_price(
            5.10, 1.0, 0.20, 100.0, 0.45, tims_dynamic_haircut, 0.0025,
        );

        // Sanity: the two bases must actually differ, else the test cannot
        // discriminate "wired" from "unwired" TIMS.
        assert!(
            (tims_reservation - static_15_reservation).abs() > 0.01,
            "Test construction error: TIMS haircut ({:.6}) too close to the static 15% \
             baseline to be distinguishable",
            tims_dynamic_haircut
        );

        // The real, still-open bug: today ActiveOrchestrator always uses the
        // static 15% haircut, so the live quote matches static_15_reservation,
        // not tims_reservation. RED until TimsMarginModel is wired in.
        assert!(
            (quote.reservation_price - tims_reservation).abs() < 1e-6,
            "Task 23 Failure: ActiveOrchestrator reservation price ({:.6}) does not reflect \
             TimsMarginModel's dynamic haircut ({:.6}) -- still using the static 15% haircut \
             (static_reservation={:.6}, tims_reservation={:.6})",
            quote.reservation_price,
            tims_dynamic_haircut,
            static_15_reservation,
            tims_reservation
        );
    }
}
