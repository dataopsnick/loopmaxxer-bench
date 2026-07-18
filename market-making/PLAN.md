# Plan: Production-Grade SOFR-Neutral Multi-Asset Market-Making Framework

## Current State Analysis

After thorough exploration, I've confirmed the following:

**Already implemented (simulation-grade, with unit tests):**
- `src/symbology.rs` — PackedAssetKey (§2) ✅
- `src/portfolio.rs` — AtomicPortfolioState, AlignedGreeksTracker (§5, §15, §20) ✅
- `src/pricer.rs` — UltraFastPricer: Hastings CDF + BS (§13) ✅
- `src/vol_surface.rs` — MonotonicCubicSplineEvaluator (§23) ✅
- `src/sofr.rs` — SOFRHedgeController, WW bands (§3, §5.3) ✅
- `src/ofi.rs` — MicrostructureOFI (§17) ✅
- `src/bookmaker.rs` — Bookmaker (§18, §29) ✅
- `src/risk_gate.rs` — PreTradeRiskGate (§24) ✅
- `src/hedging/router.rs` — HedgingRoutingMatrix (§22, §25) ✅
- `src/margin/tims.rs` — TimsMarginModel (§12) ✅
- `src/term_structure/estimator.rs` — TermStructureBetaEstimator (§14) ✅
- `src/simulation.rs`, `src/gmm/`, `src/mle/` ✅

**Issues found:**
- `src/margin/` is missing a `mod.rs` file (only has `tims.rs`)
- `hedging`, `margin`, `term_structure` modules are NOT declared in `main.rs`
- README only covers simulation-grade architecture

## Implementation Plan

### Phase 1: Wire Existing Modules & Fix Structure
1. Create `src/margin/mod.rs` to export `tims` submodule
2. Add `mod hedging;`, `mod margin;`, `mod term_structure;` declarations to `main.rs`
3. Verify `cargo build` passes with existing code

### Phase 2: New Production-Grade Modules

#### 2.1 `src/codec/` — Zero-Copy SBE/FIX Binary Serialization (§8)
- `mod.rs` — module exports
- `fix_template.rs` — Pre-allocated fixed-buffer FIX 4.4 message templates with offset-based field stuffing (no `format!`/`write!` on hot path)
- `sbe_encoder.rs` — SBE template blitting for outbound New Order Single (NOS) and mass-cancel messages
- `IoSlice`/`sendto` integration for NIC submission
- Unit tests: template stuffing, byte-level verification

#### 2.2 `src/purge/` — SQF Purge Driver & COB Legging Defense (§9)
- `mod.rs` — module exports
- `sqf_driver.rs` — `LowLatencyPurgeDriver` with 40-byte `#[repr(C, packed)]` `SQFPurgeRequest` frame, non-blocking UDP socket, zero-allocation `unsafe` pointer-cast payload
- `cob_defense.rs` — Complex Order Book (COB) `msgspreadbookquote` ingestion and asymmetric skew repositioning
- Unit tests: frame serialization, COB skew logic

#### 2.3 `src/recorder/` — Columnar mmap Time-Series Recorder (§10)
- `mod.rs` — module exports
- `mapped_writer.rs` — `MappedColumnarWriter` using `mmap` + `O_DIRECT` (Linux, gated by `cfg(target_os = "linux")`), columnar layout (u64 timestamp, f64 price, u32 size), `Drop` with `munmap` cleanup, overflow protection
- Fallback in-memory writer for macOS dev
- Unit tests: append/read, overflow, capacity

#### 2.4 `src/dropcopy/` — FIX Drop Copy Closed-Loop Listener (§19, §20)
- `mod.rs` — module exports
- `listener.rs` — `RawDropCopyListener` with zero-allocation byte-scanning FIX 4.4 parser, SOH-delimited tag extraction (Tag 35, 150, 32, 54), direct atomic CAS update of `AtomicPortfolioState.net_delta`, fragmented frame carry-over
- Unit tests: tag extraction, partial frame, fill delta update

#### 2.5 `src/ingestion/` — Hardware-Bypass Ingestion & NUMA Topology (§4, §28, §31-34)
- `mod.rs` — module exports
- `ef_vi.rs` — Solarflare EF_VI C FFI bindings (`ef_driver_open`, `ef_pd_alloc`, `ef_memreg_alloc`, `ef_vi_alloc_from_pd`, `ef_vi_rx_post`, `ef_eventq_poll`), gated behind `cfg(target_os = "linux")` and `feature = "ef_vi"`
- `dma_buffer.rs` — Page-aligned `#[repr(C, align(4096))]` DMA frame buffers with `mlock`
- `numa.rs` — NUMA-aware thread pinning via `core_affinity` crate
- `spider_stream.rs` — Zero-copy SBE/SpiderStream header overlay casting (`SpiderStreamHeader`, `StockBookQuoteBody`)
- `driver.rs` — `UserspaceIngestionDriver` with polling loop
- Unit tests: struct overlay casting, NUMA pinning (mock)

#### 2.6 `src/clearing/` — CMTA Post-Trade Clearing & Margin Sweep (§6)
- `mod.rs` — module exports
- `cmta.rs` — Multi-strike option position aggregation, step-out netting, cross-expiration compression
- `margin_sweep.rs` — SPAN/TIMS haircut minimization, EOD clearing broker API integration stub, excess margin sweep to bilateral repo / SOFR overnight deposits
- Unit tests: netting compression, sweep logic

### Phase 3: Extend Existing Modules

#### 3.1 Extend `src/vol_surface.rs` — Real-Time Taylor Expansion (§7)
- Add `TaylorVolSurface` struct with ATM-centered 2nd-order expansion coefficients
- `σ(S+ΔS, K, τ+Δτ) ≈ σ_ATM + ∂σ/∂S·ΔS + ½·∂²σ/∂S²·ΔS² + ∂σ/∂τ·Δτ`
- Background Kalman filter / Ridge-regularized OLS refit stub (ms cadence)
- Hot-path read-only coefficient vector load
- Unit tests: Taylor expansion accuracy, coefficient update

#### 3.2 Extend `src/bookmaker.rs` — Online κ Estimator (§27)
- Add `KappaEstimator` struct with sliding-window fill arrival intensity tracking
- `κ_i(t) = ln(1 + N_fills / (λ_arrival · Δt)) / D̄_spread` computation
- Per-strike dynamic spread widening when market depth thins
- Unit tests: kappa adaptation, spread widening

### Phase 4: Production Orchestrator (§16, §29)

#### 4.1 `src/orchestrator/`
- `mod.rs` — module exports
- `live_tick.rs` — `LiveMarketTick` struct with `PackedAssetKey`, spot, strike, expiry, bid/ask px/sz
- `orchestrator.rs` — `ActiveOrchestrator` integrating all subsystems into a single NUMA-pinned spin loop
- Pipeline: ingest → spline vol → reservation price + OFI → spread → risk gate → DMA submit
- 65k-capacity `ArrayQueue<LiveMarketTick>` lock-free ring buffer
- `direct_dma_submit_nos` hook for SmartNIC TX descriptor writes
- Unit tests: pipeline integration, tick processing

### Phase 5: Integration & Documentation

#### 5.1 Integration Test
- `tests/integration_test.rs` — synthetic tick stream → full pipeline → outbound quote/hedge validation

#### 5.2 Benchmark Harness
- `benches/hot_path.rs` — sub-microsecond latency benchmark for reservation pricing + risk gate + spread computation

#### 5.3 Update `Cargo.toml`
- Add `criterion` as dev-dependency for benchmarks

#### 5.4 Update `README.md`
- Add production-grade architecture table
- Document build flags: `RUSTFLAGS="-C target-cpu=native" cargo build --release`
- NUMA pinning instructions (`isolcpus`, `pthread_setaffinity_np`)
- `RLIMIT_MEMLOCK=unlimited` setup (`/etc/security/limits.conf`)
- EF_VI feature flag documentation

#### 5.5 Update `main.rs`
- Add `mod` declarations for all new modules
- Add CLI subcommand `live` for production orchestrator mode (with simulation fallback on macOS)

### Key Design Decisions

1. **Platform gating**: All Linux-specific code (EF_VI, mmap/O_DIRECT, mlock) uses `cfg(target_os = "linux")` with macOS-compatible fallbacks for development
2. **Unsafe code**: All `unsafe` blocks include `// SAFETY:` comments and are gated behind `cfg(target_os = "linux")` where platform-specific
3. **Alignment**: All hot-path risk state structs use `#[repr(align(64))]`
4. **Atomic ordering**: `Ordering::Relaxed` for hot-path reads, `Release/Acquire` for cross-thread publication, `SeqCst` only for kill-switch state
5. **No new heavy dependencies**: Hand-rolled SBE encoder, FIX parser, QP solver (existing approach)

### Files to Create/Modify

| Action | File | Spec § |
|--------|------|--------|
| Create | `src/codec/mod.rs`, `src/codec/fix_template.rs`, `src/codec/sbe_encoder.rs` | §8 |
| Create | `src/purge/mod.rs`, `src/purge/sqf_driver.rs`, `src/purge/cob_defense.rs` | §9 |
| Create | `src/recorder/mod.rs`, `src/recorder/mapped_writer.rs` | §10 |
| Create | `src/dropcopy/mod.rs`, `src/dropcopy/listener.rs` | §19, §20 |
| Create | `src/ingestion/mod.rs`, `src/ingestion/ef_vi.rs`, `src/ingestion/dma_buffer.rs`, `src/ingestion/numa.rs`, `src/ingestion/spider_stream.rs`, `src/ingestion/driver.rs` | §4, §28, §31-34 |
| Create | `src/clearing/mod.rs`, `src/clearing/cmta.rs`, `src/clearing/margin_sweep.rs` | §6 |
| Create | `src/orchestrator/mod.rs`, `src/orchestrator/live_tick.rs`, `src/orchestrator/orchestrator.rs` | §16, §29 |
| Create | `src/margin/mod.rs` | §12 |
| Modify | `src/vol_surface.rs` (add Taylor expansion) | §7 |
| Modify | `src/bookmaker.rs` (add κ estimator) | §27 |
| Modify | `src/main.rs` (add module declarations + `live` subcommand) | — |
| Modify | `Cargo.toml` (add criterion dev-dep) | — |
| Modify | `README.md` (production docs) | — |
| Create | `tests/integration_test.rs` | — |
| Create | `benches/hot_path.rs` | — |

### Risks & Mitigations

1. **macOS development**: All Linux-specific syscalls (mmap/O_DIRECT, EF_VI FFI, mlock) are `cfg`-gated with in-memory fallbacks so `cargo build`/`cargo test` works on macOS
2. **EF_VI FFI**: The `extern "C"` bindings link against `libonload` which only exists on Linux; gated behind `feature = "ef_vi"` and `cfg(target_os = "linux")`
3. **Unsafe code review**: Every `unsafe` block has a `// SAFETY:` comment explaining the invariant
4. **Clippy compliance**: All new code will pass `cargo clippy` with no warnings

---

Does this plan match your vision? If so, please **toggle to Act mode** and I'll begin implementation.