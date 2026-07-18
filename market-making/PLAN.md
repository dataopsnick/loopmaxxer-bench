# Plan: "Mr. Market" Bookmaking Simulation with GMM Hidden-State MLE Position Inference

## Overview

Build a Rust project that implements the Korean spec's market-making framework (Avellaneda-Stoikov reservation pricing, SOFR carry optimization, Whalley-Wilmott hedge bands, OFI microstructure drift, monotonic spline vol surface, pre-trade risk gates) as a **simulated** "Mr. Market" broker-dealer. The simulation replays IEX historical T&S / depth-of-book data through the engine, while a **Gaussian Mixture Model (GMM)** with 3 hidden states (noise trader, institutional buyer, informed insider) is fit via EM on observed order-flow features. The MLE position size of the delta-neutral level-IV market maker is then inferred by maximizing expected log-likelihood of P&L given the fitted mixture, adverse selection costs, and SOFR carry.

## Architecture

```
src/
├── main.rs                  # Entry point: orchestrate download → load → fit GMM → run sim → infer MLE
├── symbology.rs             # PackedAssetKey (spec §2)
├── portfolio.rs             # AtomicPortfolioState, AlignedGreeksTracker (spec §5, §15, §20)
├── pricer.rs                # UltraFastPricer: Hastings CDF, Black-Scholes (spec §13)
├── vol_surface.rs           # MonotonicCubicSplineEvaluator (spec §23)
├── sofr.rs                  # SOFRHedgeController, Whalley-Wilmott bands (spec §3, §5.3)
├── ofi.rs                   # MicrostructureOFI drift estimator (spec §17)
├── bookmaker.rs             # Reservation price + indifference spread quoting (spec §18, §29)
├── risk_gate.rs             # PreTradeRiskGate (spec §24)
├── iex/
│   ├── mod.rs
│   ├── downloader.rs        # Download IEX historical PCAP files by date/symbol
│   └── parser.rs            # Parse IEX PCAP → TOPS/DEEP/Trade messages → MarketEvent structs
├── memorydb/
│   ├── mod.rs
│   ├── client.rs            # Redis-protocol client (redis crate) for AWS MemoryDB
│   └── vector_store.rs      # Time-series vector storage + similarity query for MLE feature lookup
├── gmm/
│   ├── mod.rs
│   ├── model.rs             # 3-component GMM: noise, institutional, informed
│   ├── em.rs                # Expectation-Maximization fitting algorithm
│   └── features.rs          # Extract order-flow features (size, direction, price impact, OFI)
├── mle/
│   ├── mod.rs
│   ├── position.rs          # MLE position size inference for delta-neutral MM
│   └── likelihood.rs        # Log-likelihood: spread revenue − adverse selection − SOFR carry
└── simulation.rs            # Mr. Market replay engine: feed IEX data → quote → fill → update portfolio
```

## Key Design Decisions

### 1. IEX Historical Data (Constraint 2)
- IEX provides free historical data at `iextrading.com/trading/market-data/#hist-download` as daily PCAP files (TOPS = top-of-book, DEEP = full depth, plus trade data).
- `iex/downloader.rs` will fetch PCAP files by date via HTTP (using `reqwest`).
- `iex/parser.rs` will parse the PCAP → IEX message format (header + payload) to extract:
  - **T&S**: trade price, size, timestamp, symbol
  - **Depth of book**: bid/ask price/size at multiple levels
  - **Last sale**: trade events
- A CSV fallback reader will be included for testing without downloading full PCAPs.

### 2. AWS MemoryDB as Time-Series Vector DB (Constraint 3)
- MemoryDB is Redis-compatible; use the `redis` crate with TLS support.
- `memorydb/client.rs`: connection pool, TLS config, MemoryDB endpoint.
- `memorydb/vector_store.rs`: store market microstructure feature vectors (OFI, spread, trade imbalance, volatility) as Redis sorted sets + hash fields keyed by timestamp. Use Redis `FT.SEARCH` (RediSearch module available in MemoryDB) for vector similarity queries to find historically similar regimes for MLE positioning.
- Feature vectors: `[normalized_trade_size, signed_order_flow, OFI_ewma, spread_width, vol_atm, return_predictability]`

### 3. GMM Hidden-State Model
Three Gaussian components modeling the order-flow generating process:

| Component | Mean (signed size) | Std | Interpretation |
|-----------|-------------------|-----|----------------|
| Noise trader | ~0 | small | Symmetric, no info |
| Institutional | large positive (or negative) | medium | Persistent directional flow |
| Informed insider | medium, correlated with future returns | small | Adverse selection |

- `gmm/em.rs`: Standard EM algorithm (E-step: posterior responsibilities, M-step: update μ, σ, π) fit on historical order-flow feature vectors from IEX data.
- The fitted GMM gives posterior probabilities P(state | observed order) at each tick, which feeds into the MLE position inference.

### 4. MLE Position Size Inference
The delta-neutral market maker's expected log-likelihood per unit time:

$$\mathcal{L}(q) = \mathbb{E}[\text{spread revenue}(q)] - \mathbb{E}[\text{adverse selection}(q)] - \Phi_{\text{SOFR}}(q)$$

Where:
- **Spread revenue** ∝ κ · (fill rate) · spread_width — depends on noise-trader mixture weight π_noise
- **Adverse selection** ∝ E[|future return| · |informed flow|] — depends on informed-trader weight π_informed and their directional mean μ_informed
- **SOFR carry** = the spec's Φ_SOFR(q) function from §3.2

The MLE position q* is found by:
1. For each candidate q, compute expected log-likelihood using the fitted GMM parameters and historical conditional expectations
2. Use grid search + golden-section optimization (no external solver dependency) to find argmax
3. The result is the "level IV broker dealer" optimal inventory that balances spread harvesting vs. adverse selection vs. carry cost

### 5. Spec Engine Components (from the Korean spec)
All core components from the spec are implemented as the simulation's quoting/hedging engine:
- **PackedAssetKey** (§2): 128-bit packed symbology
- **AtomicPortfolioState** (§5, §15, §20): lock-free atomic Greeks tracking
- **UltraFastPricer** (§13): Hastings rational CDF approximation + Black-Scholes
- **MonotonicCubicSplineEvaluator** (§23): Hyman-filtered vol surface
- **SOFRHedgeController** (§3, §5.3): Whalley-Wilmott bands with SOFR drift correction
- **MicrostructureOFI** (§17): EWMA OFI drift → reservation price adjustment
- **Reservation pricing + indifference spread** (§18, §29): Avellaneda-Stoikov with SOFR penalty
- **PreTradeRiskGate** (§24): 10ns pre-trade validation

### 6. Simulation Flow (`simulation.rs`)
```
1. Download/load IEX historical data for target symbols (AAPL, NVDA, etc.)
2. Parse → stream of MarketEvent (quote updates, trades)
3. Store feature vectors in MemoryDB
4. Fit GMM on historical order-flow features (EM)
5. Replay events through Mr. Market engine:
   a. On quote update: update OFI, recompute reservation price, emit bid/ask
   b. On trade: check if our quote was crossed → simulate fill → update portfolio delta
   c. Portfolio delta drift → Whalley-Wilmott hedge evaluation → emit hedge orders
   d. Pre-trade risk gate validates every outbound order
6. After replay: compute MLE position size q* from fitted GMM + realized P&L
7. Output: optimal q*, GMM parameters, P&L attribution, SOFR carry analysis
```

## Dependencies (`Cargo.toml`)
```toml
[dependencies]
tokio = { version = "1.40", features = ["full"] }
crossbeam-queue = "0.3"
ndarray = { version = "0.15", features = ["rayon"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"
redis = { version = "0.27", features = ["tls-native-roots", "tokio-comp"] }
reqwest = { version = "0.12", features = ["blocking"] }
pcap = "2.0"
clap = { version = "4.5", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = "0.3"
rayon = "1.10"
```

## Risks & Mitigations
1. **Broken Homebrew cargo** (libgit2 mismatch): Will attempt to fix via symlink or install rustup. If neither works, code will still be correct and compilable once cargo is fixed.
2. **IEX PCAP format complexity**: The IEX PCAP format has specific message schemas. Will implement a parser based on the documented IEX message types, with a CSV fallback for testing.
3. **MemoryDB availability**: If no live MemoryDB cluster is available, will include a local Redis fallback mode for development/testing.
4. **GMM convergence**: EM can get stuck in local optima. Will use k-means++ initialization and multiple restarts.

## Deliverables
- Complete Rust workspace with all modules
- `Cargo.toml` with all dependencies
- CLI interface (via `clap`) to specify: symbols, date range, MemoryDB endpoint, SOFR rate, risk parameters
- Output report: GMM parameters, MLE position size q*, P&L breakdown, SOFR carry analysis
- README with build & run instructions

---

Does this plan match your vision? If so, please **toggle to Act mode** and I'll begin implementation.
