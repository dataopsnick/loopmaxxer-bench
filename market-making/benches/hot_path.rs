//! Sub-microsecond latency benchmark for the hot-path pipeline.
//!
//! Measures: reservation pricing + risk gate + spread computation
//! through the bookmaker, as well as the full orchestrator tick processing.
//!
//! Run with: `cargo bench --bench hot_path`

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use mr_market::bookmaker::{Bookmaker, BookmakerConfig};
use mr_market::orchestrator::{ActiveOrchestrator, LiveMarketTick, OrchestratorConfig};
use mr_market::symbology::{sources, PackedAssetKey};
use mr_market::vol_surface::TaylorVolSurface;

fn bench_bookmaker_quote(c: &mut Criterion) {
    let config = BookmakerConfig::default();
    let mut bm = Bookmaker::new(config);
    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");

    c.bench_function("bookmaker_compute_quote", |b| {
        b.iter(|| {
            let quote = bm.compute_quote(
                black_box(key),
                black_box(150.0),
                black_box(149.98),
                black_box(500.0),
                black_box(150.02),
                black_box(500.0),
                black_box(0.0),
                black_box(0.20),
                black_box(1000),
            );
            black_box(quote);
        })
    });
}

fn bench_taylor_vol_eval(c: &mut Criterion) {
    let surface = TaylorVolSurface::flat(0.20, 150.0, 0.25);

    c.bench_function("taylor_vol_evaluate", |b| {
        b.iter(|| {
            let vol = surface.evaluate_vol(
                black_box(150.0),
                black_box(150.0),
                black_box(0.25),
            );
            black_box(vol);
        })
    });
}

fn bench_orchestrator_process_tick(c: &mut Criterion) {
    let config = OrchestratorConfig::default();
    let mut orch = ActiveOrchestrator::new(config);
    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
    let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);

    c.bench_function("orchestrator_process_tick", |b| {
        b.iter(|| {
            let quote = orch.process_tick(black_box(&tick));
            black_box(quote);
        })
    });
}

fn bench_orchestrator_submit_tick(c: &mut Criterion) {
    let config = OrchestratorConfig::default();
    let orch = ActiveOrchestrator::new(config);
    let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
    let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);

    c.bench_function("orchestrator_submit_tick", |b| {
        b.iter(|| {
            // Pop the previous tick to keep the queue from filling up
            let _ = orch.tick_queue().pop();
            let result = orch.submit_tick(black_box(tick));
            black_box(result);
        })
    });
}

fn bench_sbe_nos_encode(c: &mut Criterion) {
    use mr_market::codec::sbe_encoder::SbeEncoder;
    let encoder = SbeEncoder::new();

    c.bench_function("sbe_encode_nos", |b| {
        b.iter(|| {
            let nos = encoder.encode_new_order_single(
                black_box(1),
                black_box("AAPL"),
                black_box(1),
                black_box(100),
                black_box(150.25),
                black_box(1000),
            );
            black_box(nos);
        })
    });
}

criterion_group!(
    benches,
    bench_bookmaker_quote,
    bench_taylor_vol_eval,
    bench_orchestrator_process_tick,
    bench_orchestrator_submit_tick,
    bench_sbe_nos_encode,
);

criterion_main!(benches);