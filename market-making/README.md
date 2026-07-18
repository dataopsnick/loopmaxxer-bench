# Mr. Market — Simulated Bookmaking Operation

A high-frequency market-making simulation framework implementing the Korean Order Book Specification (`korean-order-book-spec.md`). The system models a delta-neutral level-IV broker-dealer market making against noise traders, institutional buyers, and informed insiders, with MLE position size inference via a 3-component Gaussian Mixture Model (GMM) hidden state.

## Architecture

The system is implemented entirely in Rust and organized into the following modules:

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

## Pipeline

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

## Prerequisites

- **Rust** 1.75+ (stable toolchain)
- **Cargo** (comes with Rust)
- Optional: AWS MemoryDB cluster or local Redis for feature caching

## Building

```bash
cargo build --release
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

## Testing

Run the test suite:

```bash
cargo test --release
```

Run tests with verbose output:

```bash
RUST_LOG=debug cargo test --release -- --nocapture
```

## Project Structure

```
market-making/
├── Cargo.toml
├── korean-order-book-spec.md
├── README.md
└── src/
    ├── main.rs              # CLI entry point
    ├── simulation.rs        # Mr. Market replay engine
    ├── bookmaker.rs         # Quoting engine
    ├── sofr.rs              # SOFR hedge controller
    ├── ofi.rs               # OFI microstructure drift
    ├── risk_gate.rs         # Pre-trade risk gate
    ├── portfolio.rs         # Atomic portfolio Greeks
    ├── pricer.rs            # Black-Scholes pricer
    ├── vol_surface.rs       # Monotonic spline vol surface
    ├── symbology.rs         # 128-bit packed asset keys
    ├── iex/
    │   ├── mod.rs           # MarketEvent, TopOfBook types
    │   ├── downloader.rs    # IEX historical data downloader
    │   └── parser.rs        # PCAP/CSV parser
    ├── memorydb/
    │   ├── mod.rs           # MemoryDbConfig
    │   ├── client.rs        # Redis client + in-memory fallback
    │   └── vector_store.rs  # Feature vector storage & similarity
    ├── gmm/
    │   ├── mod.rs           # Module declarations
    │   ├── model.rs         # GmmModel, GmmComponent, TraderState
    │   ├── em.rs            # EM fitter
    │   └── features.rs      # Order-flow feature extraction
    └── mle/
        ├── mod.rs           # Module declarations
        ├── likelihood.rs    # Position likelihood construction
        └── position.rs      # MLE position inferer
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

## License

This project is for research and educational purposes.