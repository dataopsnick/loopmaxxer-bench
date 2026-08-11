//! Combinatorial Purged Cross-Validation (CPCV)
//!
//! Implements Marcos López de Prado's Combinatorial Purged Cross-Validation
//! procedure (*Advances in Financial Machine Learning*, Ch. 12) to measure
//! the bookmaker's VWAP slippage and price impact against real historical
//! market data across a *distribution* of chronology-respecting,
//! information-leak-free train/test splits — rather than a single,
//! easily-overfit in-sample backtest.
//!
//! ## Procedure
//!
//! 1. Partition the historical event stream into `N` contiguous groups.
//! 2. Enumerate every `C(N, k)` combination of `k` groups as the held-out
//!    test set for that combination; the remaining groups form the
//!    training set.
//! 3. **Purge** training observations whose price-history lookback window
//!    overlaps a test group (Task: this prevents the GMM order-flow model
//!    from training on data whose realized-vol/return-predictability
//!    features "see" test-set trades).
//! 4. **Embargo** a short window immediately following each test group so
//!    training data does not leak backwards through serial correlation.
//! 5. Fit the GMM hidden-state model on the purged/embargoed training
//!    set, replay the bookmaker on the held-out test set
//!    (`MrMarketSimulation::run_oos`), and compute VWAP slippage and
//!    price-impact λ on that genuinely out-of-sample fold.
//! 6. Reconstruct `φ = C(N-1, k-1)` complete, non-overlapping backtest
//!    paths from the individual combinations, giving an empirical
//!    *distribution* of OOS performance instead of one lucky/unlucky
//!    number.
//!
//! See: López de Prado, M. (2018). *Advances in Financial Machine
//! Learning*, Chapter 12: Backtesting Through Cross-Validation.

pub mod evaluator;
pub mod metrics;
pub mod splitter;

pub use evaluator::{CpcvEvaluator, CpcvReport, CpcvSummary, PathResult};
pub use metrics::{
    bookmaker_implied_impact, compute_vwap, estimate_price_impact, vwap_slippage,
    PriceImpactEstimate, VwapSlippageStats,
};
pub use splitter::{CpcvConfig, CpcvFold, CpcvSplitter};
