# Title

Implement Korean Order Book Spec: Production-Grade SOFR-Neutral Multi-Asset Market-Making Framework

## Description

### Summary

Implement the full `korean-order-book-spec.md` (34 sections) as a production-grade Rust framework for a market-neutral, SOFR-cost-optimized, multi-asset spread-harvesting and bookmaking engine. The current codebase (`mr_market`) already contains a **simulation-grade** subset of the spec (symbology, atomic portfolio state, Hastings pricer, monotonic spline, SOFR hedge controller, OFI drift, bookmaker, pre-trade risk gate, plus GMM/MLE/simulation tooling). This issue tracks the remaining **production-grade** components required for live, sub-microsecond, hardware-bypass operation as specified in the Korean spec.

### Background

The spec describes an ultra-low-latency trading system that:

- Harvests spreads across equities, bonds, options, and futures while maintaining market neutrality
- Minimizes overnight SOFR carry cost and clearing margin lock-up
- Uses kernel-bypass networking (Solarflare EF_VI / DPDK), lock-free NUMA-local data structures, and cache-line-aligned atomics
- Applies SOFR-biased Avellaneda-Stoikov reservation pricing with Whalley-Wilmott hedge bands
- Enforces no-arbitrage constraints on the volatility surface (Hyman monotonic cubic spline)
- Includes pre-trade risk gates, mass-cancel (SQF purge) drivers, and closed-loop FIX drop-copy risk feedback

### Current State (Already Implemented)

The following spec components exist in simulation-grade form and are covered by unit tests:

| Spec Section | Module | Status |
|:---|:---|:---|
| §2 — 128-bit Packed Symbology | `src/symbology.rs` | ✅ Implemented |
| §3, §5.3 — SOFR Hedge Controller | `src/sofr.rs` | ✅ Implemented |
| §5, §15, §20 — Atomic Portfolio State | `src/portfolio.rs` | ✅ Implemented |
| §13 — Ultra-Fast Pricer (Hastings CDF + BS) | `src/pricer.rs` | ✅ Implemented |
| §17 — OFI Microstructure Drift | `src/ofi.rs` | ✅ Implemented |
| §18, §29 — Bookmaker (Reservation + Spread) | `src/bookmaker.rs` | ✅ Implemented |
| §23 — Monotonic Cubic Spline Vol Surface | `src/vol_surface.rs` | ✅ Implemented |
| §24 — Pre-Trade Risk Gate | `src/risk_gate.rs` | ✅ Implemented |
| Simulation engine + GMM + MLE | `src/simulation.rs`, `src/gmm/`, `src/mle/` | ✅ Implemented |

### Work Required

The following spec sections are **not yet implemented** and must be built to achieve full spec compliance. Each item maps to one or more new Rust modules.

#### 1. Hardware-Bypass Ingestion & NUMA Topology (§4, §28, §31–34)

- [ ] Implement `UserspaceIngestionDriver` with Solarflare EF_VI C FFI bindings (`ef_driver_open`, `ef_pd_alloc`, `ef_memreg_alloc`, `ef_vi_alloc_from_pd`, `ef_vi_rx_post`, `ef_eventq_poll`)
- [ ] Page-aligned (`#[repr(C, align(4096))]`) DMA frame buffers with `mlock` + IOMMU registration
- [ ] NUMA-aware thread pinning (`pthread_setaffinity_np` / `core_affinity` crate) for ingestion (NUMA 0) and pricing (NUMA 1) threads
- [ ] Zero-copy SBE/SpiderStream header overlay casting (`SpiderStreamHeader`, `StockBookQuoteBody`)
- [ ] Document `isolcpus`, `RLIMIT_MEMLOCK=unlimited`, and `-C target-cpu=native` deployment requirements

**Target module:** `src/ingestion/` (new)

#### 2. Zero-Copy SBE/FIX Binary Serialization (§8)

- [ ] Pre-allocated fixed-buffer FIX 4.4 message templates with offset-based field stuffing (no `format!`/`write!` on hot path)
- [ ] SBE template blitting for outbound New Order Single (NOS) and mass-cancel messages
- [ ] Direct `IoSlice` / `sendto` integration for NIC submission

**Target module:** `src/codec/` (new)

#### 3. SQF Purge Driver & COB Legging Defense (§9)

- [ ] `LowLatencyPurgeDriver` with 40-byte `#[repr(C, packed)]` `SQFPurgeRequest` frame
- [ ] Non-blocking UDP socket, zero-allocation `unsafe` pointer-cast payload submission
- [ ] Complex Order Book (COB) `msgspreadbookquote` ingestion and asymmetric skew repositioning to defend against legging-out arbitrage

**Target module:** `src/purge/` (new)

#### 4. Columnar mmap Time-Series Recorder (§10)

- [ ] `MappedColumnarWriter` using `mmap` + `O_DIRECT` for zero-syscall trade logging
- [ ] Columnar layout: `u64` timestamp, `f64` price, `u32` size arrays
- [ ] `Drop` implementation with `munmap` cleanup
- [ ] Overflow protection and capacity pre-allocation

**Target module:** `src/recorder/` (new)

#### 5. Real-Time TIMS/SPAN Margin Modeling (§12)

- [ ] 17-scenario TIMS stress grid (±3%, ±8%, ±15% spot moves × ±10% vol shifts)
- [ ] Cross-asset delta/gamma netting for non-linear margin reduction
- [ ] Negative margin cost weighting for risk-reducing hedge orders
- [ ] SPAN margin opportunity-cost tracking for futures positions

**Target module:** `src/margin/` (new)

#### 6. Term-Structure Beta Estimator (§14)

- [ ] `TermStructureBetaEstimator` with EWMA covariance/variance accumulators
- [ ] Atomic CAS dual-slot update for lock-free concurrent updates
- [ ] Contango/backwardation regime detection and inventory penalty widening

**Target module:** `src/term_structure/` (new)

#### 7. Real-Time Vol Surface Taylor Expansion (§7)

- [ ] ATM-centered local Taylor 2nd-order expansion: `σ(S+ΔS, K, τ+Δτ) ≈ σ_ATM + ∂σ/∂S·ΔS + ½·∂²σ/∂S²·ΔS² + ∂σ/∂τ·Δτ`
- [ ] Background Kalman filter / Ridge-regularized OLS refit thread (ms cadence)
- [ ] Hot-path read-only coefficient vector load for per-strike implied vol computation

**Target module:** `src/vol_surface.rs` (extend)

#### 8. Smart Hedging Routing Matrix (§22, §25)

- [ ] `HedgingRoutingMatrix` quadratic-programming solver for optimal stock/future/basket hedge allocation
- [ ] Objective: `min{ γ·hᵀΣh + hᵀc_spread + hᵀr_carry }` subject to `wᵀh + ΔD = 0`
- [ ] Real-time covariance matrix, slippage coefficients, and overnight carry cost vectors

**Target module:** `src/hedging/` (new)

#### 9. FIX Drop Copy Closed-Loop Listener (§19, §20)

- [ ] `RawDropCopyListener` with zero-allocation byte-scanning FIX 4.4 parser
- [ ] SOH-delimited tag extraction (Tag 35, 150, 32, 54) without `String` allocation
- [ ] Direct atomic CAS update of `AtomicPortfolioState.net_delta` within <5µs of fill
- [ ] Fragmented frame carry-over handling for partial TCP reads

**Target module:** `src/dropcopy/` (new)

#### 10. Online κ (Liquidity Parameter) Estimator (§27)

- [ ] Sliding-window fill arrival intensity tracking
- [ ] Adaptive `κ_i(t) = ln(1 + N_fills / (λ_arrival · Δt)) / D̄_spread` computation
- [ ] Per-strike dynamic spread widening when market depth thins

**Target module:** `src/bookmaker.rs` (extend)

#### 11. Production Orchestrator (§16, §29)

- [ ] `RealTimeBookMaker` / `ActiveOrchestrator` integrating all subsystems into a single NUMA-pinned spin loop
- [ ] Pipeline: ingest → spline vol → reservation price + OFI → spread → risk gate → DMA submit
- [ ] `LiveMarketTick` struct with `PackedAssetKey`, spot, strike, expiry, bid/ask px/sz
- [ ] 65k-capacity `ArrayQueue<LiveMarketTick>` lock-free ring buffer between ingestion and pricing threads
- [ ] `direct_dma_submit_nos` hook for SmartNIC TX descriptor writes

**Target module:** `src/orchestrator/` (new)

#### 12. CMTA Post-Trade Clearing & Margin Sweep (§6)

- [ ] Multi-strike option position aggregation and step-out netting
- [ ] SPAN/TIMS haircut minimization via cross-expiration compression
- [ ] End-of-day clearing broker API integration for excess margin sweep to bilateral repo / SOFR overnight deposits

**Target module:** `src/clearing/` (new)

### Acceptance Criteria

- [ ] All new modules compile under `cargo build --release` with `-C target-cpu=native -C opt-level=3`
- [ ] Unit tests for each new component (spline, risk gate, purge driver, drop-copy parser, margin grid, beta estimator, hedging router, κ estimator)
- [ ] Integration test: synthetic tick stream → full pipeline → outbound quote/hedge validation
- [ ] `cargo clippy` passes with no warnings on new code
- [ ] Documentation comments referencing spec section numbers (e.g., `//! (Spec §9)`)
- [ ] `README.md` updated with build flags, NUMA pinning, and `RLIMIT_MEMLOCK` instructions
- [ ] Benchmark harness demonstrating sub-microsecond hot-path latency for reservation pricing + risk gate + spread computation

### Technical Notes

- **Platform:** Linux x86_64 (EF_VI / DPDK / `libc` syscalls are POSIX-only). macOS development is supported for simulation modules; production hardware-bypass modules require Linux.
- **Unsafe code:** The spec mandates `unsafe` pointer casting for zero-copy DMA and binary frame overlay. All `unsafe` blocks must include safety comments and be gated behind `cfg(target_os = "linux")` where platform-specific.
- **Dependencies to add:** `libc` (mmap, mlock, mlockall), `core_affinity` (NUMA pinning), optional `sbe_codec` or hand-rolled SBE encoder.
- **Alignment:** All hot-path risk state structs must use `#[repr(align(64))]` to prevent false sharing (§15).
- **Atomic ordering:** Use `Ordering::Relaxed` for hot-path reads, `Ordering::Release/Acquire` for cross-thread publication, `Ordering::SeqCst` only for kill-switch state.

### References

- Spec file: `korean-order-book-spec.md` (2477 lines, 34 sections)
- Existing plan: `PLAN.md`
- Existing task list: `TASKLIST.md`
- Related modules: `src/symbology.rs`, `src/portfolio.rs`, `src/pricer.rs`, `src/vol_surface.rs`, `src/sofr.rs`, `src/ofi.rs`, `src/bookmaker.rs`, `src/risk_gate.rs`, `src/simulation.rs`

### Suggested Labels

`enhancement`, `spec-compliance`, `hft`, `rust`, `production`