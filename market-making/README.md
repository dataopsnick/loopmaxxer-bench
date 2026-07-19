# Mr. Market — SOFR-Neutral Multi-Asset Market-Making Framework

A production-grade, sub-microsecond market-making framework implementing the Korean Order Book Specification (`korean-order-book-spec.md`). The system models a delta-neutral level-IV broker-dealer market making against noise traders, institutional buyers, and informed insiders, with MLE position size inference via a 3-component Gaussian Mixture Model (GMM) hidden state.

## Architecture

The system is implemented entirely in Rust and organized into two tiers:

### Simulation-Grade Modules (with unit tests)

| Module | Spec Section | Description |
|--------|-------------|-------------|
| `symbology` | §2 | 128-bit packed asset symbology for equities, bonds, options, futures |
| `portfolio` | §5, §15, §20 | Atomic lock-free portfolio Greeks tracking with 64-byte cache-line alignment |
| `pricer` | §13 | Hastings rational CDF approximation + Black-Scholes option pricer |
| `vol_surface` | §23 | Hyman monotonic cubic spline volatility surface (8-node) |
| `sofr` | §3, §5.3 | SOFR-biased Whalley-Wilmott hedge bands + Avellaneda-Stoikov reservation pricing |
| `ofi` | §17 | Order Flow Imbalance (OFI) microstructure drift EWMA estimator |
| `risk_gate` | §24 | Pre-trade risk gate with kill switch (price, qty, delta limits) |
| `bookmaker` | §18, §29 | Reservation price + indifference spread quoting engine |
| `iex` | — | IEX historical PCAP parser + CSV fallback + downloader |
| `memorydb` | — | AWS MemoryDB (Redis-compatible) client + in-memory fallback + vector store |
| `gmm` | — | 3-component GMM (noise/institutional/informed) with EM fitting |
| `mle` | — | MLE position inference via grid search + golden-section optimization |
| `simulation` | — | Mr. Market replay engine orchestrating the full pipeline |

### Production-Grade Modules (zero-copy, NUMA-pinned, hardware-bypass)

| Module | Spec Section | Description |
|--------|-------------|-------------|
| `codec` | §8 | Zero-copy SBE/FIX binary serialization (pre-allocated templates, no `format!` on hot path) |
| `purge` | §9 | SQF purge driver (40-byte UDP mass-cancel) + COB legging defense |
| `recorder` | §10 | Columnar mmap time-series recorder (`O_DIRECT` on Linux, in-memory fallback on macOS) |
| `dropcopy` | §19, §20 | FIX drop-copy closed-loop listener (zero-allocation byte-scanning parser) |
| `ingestion` | §4, §28, §31-34 | Hardware-bypass ingestion: EF_VI FFI, DMA buffers, NUMA pinning, SpiderStream overlay |
| `clearing` | §6 | CMTA post-trade clearing & margin sweep (SPAN/TIMS haircut minimization) |
| `orchestrator` | §16, §29 | NUMA-pinned spin loop integrating all subsystems (65k-capacity lock-free ring buffer) |
| `hedging` | §22, §25 | Hedging routing matrix (QP solver for stock/future/basket allocation) |
| `margin` | §12 | TIMS/SPAN margin modeling (17-scenario stress grid) |
| `term_structure` | §14 | Term structure beta estimator |

### Extended Modules

| Module | Spec Section | Extension |
|--------|-------------|-----------|
| `vol_surface` | §7 | `TaylorVolSurface`: ATM-centered 2nd-order Taylor expansion with background Kalman/OLS refit |
| `bookmaker` | §27 | `KappaEstimator`: Online κ estimator with sliding-window fill arrival intensity tracking |

## Pipeline

### Simulation Pipeline

```
IEX Data → Parse → Extract Features → Store in MemoryDB → Fit GMM (EM)
                                                         ↓
                                                    Replay Events
                                                    ↓
                                              Bookmaker Quotes
                                              ↓
                                          Simulate Fills
                                          ↓
                                    Update Portfolio Delta
                                    ↓
                              WW Hedge Band Evaluation
                              ↓
                          MLE Position Inference
                          ↓
                     P&L Breakdown + Report
```

### Live Production Pipeline

```
NIC (EF_VI) → DMA Frame → SpiderStream Overlay → Lock-Free Ring Buffer (65k)
                                                              ↓
                                                    ActiveOrchestrator (NUMA-pinned)
                                                    ↓
                                              1. Taylor Vol Surface Lookup
                                              2. Reservation Price (SOFR-biased A-S)
                                              3. OFI Microstructure Drift
                                              4. Indifference Spread (dynamic κ)
                                              5. Pre-Trade Risk Gate
                                              6. SBE NOS Encoding → DMA Submit
                                              7. Whalley-Wilmott Hedge Evaluation
                                                    ↓
                                              Hedging Routing Matrix
                                              ↓
                                          Drop-Copy Fill Confirmation
                                          ↓
                                      Atomic Portfolio State Update
```

## Prerequisites

- **Rust** 1.75+ (stable toolchain)
- **Cargo** (comes with Rust)
- Optional: AWS MemoryDB cluster or local Redis for feature caching
- **Linux** (for production EF_VI / mmap / NUMA pinning)
- **Solarflare** NIC with OpenOnload (for EF_VI hardware bypass)

## Building

### Development (macOS)

```bash
cargo build --release
```

All Linux-specific code (EF_VI, mmap/O_DIRECT, mlock) is `cfg`-gated with macOS-compatible in-memory fallbacks.

### Production (Linux)

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

With EF_VI hardware bypass:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release --features ef_vi
```

The release binary is optimized with LTO and `codegen-units = 1` for maximum performance.

## Running

### Quick Synthetic Test

Run a quick simulation with synthetic market data (no IEX files needed):

```bash
cargo run --release -- test --n-events 1000
```

### Full Simulation with Synthetic Data

```bash
cargo run --release -- run \
  --symbol AAPL \
  --adv 10000000 \
  --sofr 0.0535 \
  --gamma 0.015 \
  --n-events 5000 \
  --fill-prob 0.3
```

### Simulation with IEX Historical Data

First, download IEX PCAP data:

```bash
cargo run --release -- download --date 20240115 --feed tops --output-dir ./data/iex
```

Then run the simulation with the downloaded file:

```bash
cargo run --release -- run \
  --symbol AAPL \
  --data-file ./data/iex/20240115_TOPS.pcap.gz \
  --output report.json
```

### Live Production Orchestrator

On macOS (synthetic ticks for development):

```bash
cargo run --release -- live --symbol AAPL --n-ticks 10000
```

On Linux with EF_VI (production hardware bypass):

```bash
cargo run --release -- live --symbol AAPL --pin-core --features ef_vi
```

### Using AWS MemoryDB

Connect to an AWS MemoryDB cluster for feature vector storage:

```bash
cargo run --release -- run \
  --symbol AAPL \
  --memorydb-endpoint clustercfg.my-cluster.xxxxxx.memorydb.us-east-1.amazonaws.com \
  --memorydb-port 6379 \
  --memorydb-tls \
  --memorydb-token YOUR_AUTH_TOKEN
```

If MemoryDB is unavailable, the system automatically falls back to an in-memory store.

## CLI Options

### `run` — Full Simulation

| Flag | Default | Description |
|------|---------|-------------|
| `-s, --symbol` | `AAPL` | Symbol to simulate |
| `-v, --adv` | `10000000` | Average daily volume (shares) |
| `-r, --sofr` | `0.0535` | SOFR base rate |
| `-g, --gamma` | `0.015` | Risk aversion parameter γ |
| `--margin-haircut` | `0.15` | Margin haircut |
| `--borrow-premium` | `0.0025` | Borrow premium |
| `-k, --kappa` | `2.1` | Liquidity parameter κ |
| `-t, --time-to-horizon` | `0.45` | Time to horizon (fraction of day) |
| `--max-qty` | `1000` | Max order quantity (risk gate) |
| `--max-price` | `5000` | Max price USD (risk gate) |
| `--max-delta` | `5000` | Max absolute delta (risk gate) |
| `--q-min` | `-5000` | MLE grid search lower bound |
| `--q-max` | `5000` | MLE grid search upper bound |
| `--n-grid` | `200` | MLE grid resolution |
| `--fill-prob` | `0.3` | Fill probability when quote is crossed |
| `-f, --data-file` | — | Path to IEX PCAP/CSV file |
| `-n, --n-events` | `5000` | Synthetic events (if no data file) |
| `--memorydb-endpoint` | — | MemoryDB cluster endpoint |
| `--memorydb-port` | `6379` | MemoryDB port |
| `--memorydb-tls` | `false` | Use TLS for MemoryDB |
| `--memorydb-token` | — | MemoryDB auth token |
| `-o, --output` | — | Output JSON report path |

### `live` — Live Production Orchestrator

| Flag | Default | Description |
|------|---------|-------------|
| `-s, --symbol` | `AAPL` | Symbol to make markets in |
| `-r, --sofr` | `0.0535` | SOFR base rate |
| `-g, --gamma` | `0.015` | Risk aversion parameter γ |
| `-k, --kappa` | `2.1` | Liquidity parameter κ |
| `--max-qty` | `1000` | Max order quantity (risk gate) |
| `--max-price` | `5000` | Max price USD (risk gate) |
| `--max-delta` | `5000` | Max absolute delta (risk gate) |
| `--pin-core` | `false` | Pin orchestrator thread to CPU core |
| `--n-ticks` | `10000` | Synthetic ticks (macOS dev mode) |

### `download` — Download IEX Data

| Flag | Default | Description |
|------|---------|-------------|
| `-d, --date` | — | Date in YYYYMMDD format |
| `-f, --feed` | `tops` | Feed type: `tops` or `deep` |
| `-o, --output-dir` | `./data/iex` | Output directory |

### `test` — Quick Synthetic Test

| Flag | Default | Description |
|------|---------|-------------|
| `-n, --n-events` | `1000` | Number of synthetic events |

## Key Concepts

### GMM Hidden State (3-Component)

The order-flow generating process is modeled as a mixture of three trader types:

1. **Noise Trader** (π₀): Symmetric, no directional information. Source of spread revenue.
2. **Institutional** (π₁): Persistent directional flow. Creates inventory pressure.
3. **Informed Insider** (π₂): Adverse selection, correlated with future returns. Source of adverse selection cost.

### MLE Position Inference

The optimal position size q* is found by maximizing the expected log-likelihood:

```
L(q) = E[spread_revenue(q)] - E[adverse_selection(q)] - Φ_SOFR(q) - inventory_risk(q)
```

Optimization uses grid search (200 points) followed by golden-section refinement.

### Avellaneda-Stoikov Reservation Pricing

```
R = S - γ·σ²·q·T - (SOFR + margin + borrow)·T·sign(q)
```

### Whalley-Wilmott Hedge Bands

```
band_width = (1.5 · γ·Γ²·δ / (S·σ²))^(1/3) · (1 + SOFR_drift_factor)
```

### Taylor Vol Surface (Spec §7)

ATM-centered 2nd-order expansion for real-time vol lookup:

```
σ(S+ΔS, K, τ+Δτ) ≈ σ_ATM + ∂σ/∂S·ΔS + ½·∂²σ/∂S²·ΔS² + ∂σ/∂τ·Δτ
```

Coefficients updated by background Kalman filter / Ridge-regularized OLS at millisecond cadence.

### Online κ Estimator (Spec §27)

```
κ_i(t) = ln(1 + N_fills / (λ_arrival · Δt)) / D̄_spread
```

Sliding-window fill arrival intensity tracking with per-strike dynamic spread widening when market depth thins.

## Production Setup (Linux)

### NUMA Pinning

Isolate CPU cores for the hot-path threads:

```bash
# Kernel boot parameters
isolcpus=2,3,4 nohz_full=2,3,4 rcu_nocbs=2,3,4
```

Thread assignments (via `NumaConfig`):
- Core 2: RX / ingestion thread (EF_VI polling)
- Core 3: Processing / orchestration thread (pricing, risk, quoting)
- Core 4: Drop-copy listener thread (FIX fill confirmation)

### RLIMIT_MEMLOCK

For EF_VI DMA buffers and `mlock`:

```bash
# /etc/security/limits.conf
*    soft    memlock    unlimited
*    hard    memlock    unlimited
```

### EF_VI Feature Flag

The EF_VI FFI bindings link against `libonload` (Solarflare OpenOnload), which only exists on Linux:

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release --features ef_vi
```

## Testing

Run the test suite:

```bash
cargo test --release
```

Run tests with verbose output:

```bash
RUST_LOG=debug cargo test --release -- --nocapture
```

Run integration tests only:

```bash
cargo test --release --test integration_test
```

## Benchmarking

Sub-microsecond latency benchmarks for the hot-path pipeline:

```bash
cargo bench --bench hot_path
```

Benchmarks:
- `bookmaker_compute_quote` — reservation pricing + risk gate + spread computation
- `taylor_vol_evaluate` — Taylor expansion vol lookup
- `orchestrator_process_tick` — full orchestrator pipeline (vol → quote → NOS → hedge)
- `orchestrator_submit_tick` — lock-free ring buffer enqueue
- `sbe_encode_nos` — SBE New Order Single encoding

## Project Structure

```
market-making/
├── Cargo.toml
├── korean-order-book-spec.md
├── README.md
├── PLAN.md
├── benches/
│   └── hot_path.rs              # Sub-µs latency benchmarks
├── tests/
│   └── integration_test.rs      # Full pipeline integration tests
└── src/
    ├── main.rs                  # CLI entry point (run, download, test, live)
    ├── simulation.rs            # Mr. Market replay engine
    ├── bookmaker.rs             # Quoting engine + KappaEstimator (§27)
    ├── sofr.rs                  # SOFR hedge controller
    ├── ofi.rs                   # OFI microstructure drift
    ├── risk_gate.rs             # Pre-trade risk gate
    ├── portfolio.rs             # Atomic portfolio Greeks
    ├── pricer.rs                # Black-Scholes pricer
    ├── vol_surface.rs           # Monotonic spline + Taylor vol surface (§7)
    ├── symbology.rs             # 128-bit packed asset keys
    ├── codec/                   # Zero-copy SBE/FIX serialization (§8)
    │   ├── mod.rs
    │   ├── fix_template.rs
    │   └── sbe_encoder.rs
    ├── purge/                   # SQF purge + COB defense (§9)
    │   ├── mod.rs
    │   ├── sqf_driver.rs
    │   └── cob_defense.rs
    ├── recorder/                # Columnar mmap recorder (§10)
    │   ├── mod.rs
    │   └── mapped_writer.rs
    ├── dropcopy/                # FIX drop-copy listener (§19, §20)
    │   ├── mod.rs
    │   └── listener.rs
    ├── ingestion/               # Hardware-bypass ingestion (§4, §28, §31-34)
    │   ├── mod.rs
    │   ├── ef_vi.rs
    │   ├── dma_buffer.rs
    │   ├── numa.rs
    │   ├── spider_stream.rs
    │   └── driver.rs
    ├── clearing/                # CMTA clearing & margin sweep (§6)
    │   ├── mod.rs
    │   ├── cmta.rs
    │   └── margin_sweep.rs
    ├── orchestrator/            # Production orchestrator (§16, §29)
    │   ├── mod.rs
    │   ├── live_tick.rs
    │   └── orchestrator.rs
    ├── hedging/                 # Hedging routing matrix (§22, §25)
    │   ├── mod.rs
    │   └── router.rs
    ├── margin/                  # TIMS/SPAN margin (§12)
    │   ├── mod.rs
    │   └── tims.rs
    ├── term_structure/          # Term structure beta (§14)
    │   ├── mod.rs
    │   └── estimator.rs
    ├── iex/                     # IEX data downloader + parser
    │   ├── mod.rs
    │   ├── downloader.rs
    │   └── parser.rs
    ├── memorydb/                # MemoryDB client + vector store
    │   ├── mod.rs
    │   ├── client.rs
    │   └── vector_store.rs
    ├── gmm/                     # GMM hidden-state model
    │   ├── mod.rs
    │   ├── model.rs
    │   ├── em.rs
    │   └── features.rs
    └── mle/                     # MLE position inference
        ├── mod.rs
        ├── likelihood.rs
        └── position.rs
```

## IEX Historical Data

IEX provides free historical PCAP files containing TOPS (top-of-book) and DEEP (depth-of-book) market data:

- **Source**: https://iextrading.com/trading/market-data/#hist-download
- **Format**: Gzipped PCAP with IEX message protocol
- **Message types**: Quote updates (Q), Trades (T), Price level updates (8)

The parser also supports a CSV fallback format for testing:

```csv
timestamp_ns,symbol,event_type,bid_price,bid_size,ask_price,ask_size,trade_price,trade_size
1000,AAPL,Q,149.98,500,150.02,500,,
2000,AAPL,T,,,,,150.00,100
```

## Design Decisions

1. **Platform gating**: All Linux-specific code (EF_VI, mmap/O_DIRECT, mlock) uses `cfg(target_os = "linux")` with macOS-compatible fallbacks
2. **Unsafe code**: All `unsafe` blocks include `// SAFETY:` comments and are gated behind `cfg(target_os = "linux")` where platform-specific
3. **Alignment**: All hot-path risk state structs use `#[repr(align(64))]`
4. **Atomic ordering**: `Ordering::Relaxed` for hot-path reads, `Release/Acquire` for cross-thread publication, `SeqCst` only for kill-switch state
5. **No new heavy dependencies**: Hand-rolled SBE encoder, FIX parser, QP solver

## Execution Paradigms & Setup Options

Depending on your local system resource constraints, development environment, and deployment targets, you can choose from four distinct execution paradigms:

### Option A: Cloud Sandbox (Zero Local Storage Footprint)
* **Best for:** Developers with constrained local disk space (e.g., < 5MB of free storage) or those looking for immediate, headless onboarding.
* **Infrastructure:** GitHub Codespaces (runs on a secure, remote virtual machine).
* **Configuration:** Handled automatically via `.devcontainer/` at the repository root.

#### How to Launch:
1. Go to your repository on GitHub.
2. Click the green **Code** button, select the **Codespaces** tab, and click **Create codespace on main**.
3. Once the terminal loads in your browser, run:
   ```bash
   cd market-making
   # Run the integration test suite
   cargo test --release
   # Run the simulation against the sidecarred Redis container
   cargo run --release -- run --symbol AAPL --n-events 1000 --memorydb-endpoint memorydb-emu
   ```

---

### Option B: Local Containerized (Docker Compose)
* **Best for:** Local environment isolation. Avoids installing Rust, Cargo, or system dependencies directly on your host operating system.
* **Infrastructure:** Docker Desktop or local Docker engine.
* **Configuration:** Managed via `Dockerfile.dev` and `docker-compose.yml`.

#### How to Run:
1. Start the workspace and local Redis (MemoryDB) services:
   ```bash
   docker compose up -d
   ```
2. Run the test suite inside the container:
   ```bash
   docker compose exec workspace cargo test --release
   ```
3. Run the simulation against the bridge-networked MemoryDB emulator:
   ```bash
   docker compose exec workspace cargo run --release -- run \
     --symbol AAPL \
     --n-events 1000 \
     --memorydb-endpoint memorydb-emu
   ```
   *Cargo dependencies and build targets are cached on your host using Docker named volumes (`cargo-cache` and `target-cache`) to ensure subsequent builds compile rapidly.*

---

### Option C: Serverless / Cloud-Native (AWS SAM)
* **Best for:** Testing code in a production-ready AWS Lambda execution environment.
* **Infrastructure:** AWS SAM CLI + Docker (compiles inside a container, eliminating local Rust toolchain requirements).
* **Configuration:** Managed via `template.yaml` using `PackageType: Image` and the `provided.al2023` custom runtime.

#### How to Run:
1. Start your local MemoryDB emulator:
   ```bash
   docker run -d --name local-memorydb -p 6379:6379 redis:alpine
   ```
2. Build the serverless container image:
   ```bash
   sam build
   ```
3. Invoke the local lambda function, mapping the host's Redis port into the SAM runtime:
   ```bash
   sam local invoke MarketMakerFunction \
     --env-vars env.json \
     --event <(echo '{"args": ["run", "--symbol", "AAPL", "--n-events", "500", "--memorydb-endpoint", "host.docker.internal"]}')
   ```

---

### Option D: Bare Metal / Native (Local Toolchain)
* **Best for:** Maximum raw performance, profiling, and sub-microsecond latency testing.
* **Infrastructure:** Local terminal.
* **Prerequisites:** Rust 1.80+ (stable), `pkg-config`, `libssl-dev`, and `build-essential`.

#### How to Run:
1. Ensure a local Redis server is running (simulating MemoryDB):
   ```bash
   redis-server --port 6379 &
   ```
2. Run unit and integration tests:
   ```bash
   cargo test --release
   ```
3. Execute the hot-path latency benchmarks:
   ```bash
   cargo bench --bench hot_path
   ```
4. Run the production-grade orchestrator:
   ```bash
   cargo run --release -- live --symbol AAPL --n-ticks 10000
   ```

---

## Local Cloud Emulation Reference

To avoid the overhead and cost of deploying live resources in AWS during development, this module supports several local emulation options:

### 1. Redis Alpine (Port 6379)
* **Role:** Emulates AWS MemoryDB/ElastiCache.
* **Usage:** Used as the primary key-value store for storing order-flow and microstructure feature vectors. Run headlessly in-memory for optimal execution speed:
  ```bash
  docker run -d -p 6379:6379 redis:alpine redis-server --save "" --appendonly no
  ```

### 2. floci (Port 4566)
* **Role:** A lightweight, native AWS emulator.
* **Usage:** Built with Quarkus Native, it serves as a lightweight alternative to LocalStack, supporting S3, SQS, SNS, and DynamoDB on port 4566 with a 24ms startup time and a 13MB idle memory footprint. Ideal for continuous integration testing without internet access or SaaS tokens:
  ```bash
  docker run -d -p 4566:4566 floci/floci:latest
  ```

### 3. LocalStack (Port 4566)
* **Role:** Standard AWS emulation.
* **Usage:** Serves as a heavier, feature-complete alternative to floci if full AWS API parity is required.

## License

This project is for research and educational purposes.