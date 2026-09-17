//! CPCV Evaluator
//!
//! Ties the combinatorial splitter (`cpcv::splitter`) and the
//! VWAP-slippage / price-impact metrics (`cpcv::metrics`) together with
//! the existing `MrMarketSimulation::run_oos` replay engine to produce a
//! full Combinatorial Purged Cross-Validation report: for every `C(N,k)`
//! combination, the GMM is fit strictly on the purged & embargoed
//! training set and the bookmaker is replayed strictly on the held-out
//! test set, after which real-vs-simulated VWAP slippage and price
//! impact are computed. Combination-level results are then stitched into
//! `φ = C(N-1,k-1)` complete, non-overlapping backtest paths, yielding an
//! empirical *distribution* of out-of-sample performance rather than a
//! single, easily-overfit point estimate.

use crate::cpcv::metrics::{
    bookmaker_implied_impact, compute_vwap, estimate_price_impact, vwap_slippage,
    PriceImpactEstimate, VwapSlippageStats,
};
use crate::cpcv::splitter::{CpcvConfig, CpcvSplitter};
use crate::gmm::features::extract_features;
use crate::iex::MarketEvent;
use crate::memorydb::vector_store::VectorStore;
use crate::simulation::{MrMarketSimulation, SimulationConfig};
use serde::Serialize;
use tracing::info;

/// Result of a single `C(N,k)` combination's out-of-sample evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct CombinationResult {
    /// Index of this combination in `CpcvSplitter::combinations()`.
    pub combination_index: usize,
    /// Group indices held out as the test set for this combination.
    pub test_groups: Vec<usize>,
    /// Number of training / test events used.
    pub n_train_events: usize,
    pub n_test_events: usize,
    /// Real historical VWAP slippage of the bookmaker's OOS fills.
    pub vwap_slippage: VwapSlippageStats,
    /// Real historical price-impact λ estimated from the test set's own trades.
    pub real_price_impact: PriceImpactEstimate,
    /// Price-impact λ implied by the bookmaker's simulated OOS fills.
    pub simulated_price_impact: PriceImpactEstimate,
    /// `simulated_price_impact.lambda - real_price_impact.lambda`: positive
    /// means the bookmaker's simulated market impact is *steeper* (worse)
    /// than what is observed in real historical trading.
    pub lambda_delta: f64,
}

/// A reconstructed backtest path: one combination-result per group,
/// stitched together via `CpcvSplitter::path_assignment()`.
#[derive(Debug, Clone, Serialize)]
pub struct PathResult {
    pub path_index: usize,
    /// Notional-weighted VWAP slippage (bps), averaged across the
    /// combinations assigned to this path.
    pub mean_slippage_bps: f64,
    /// Mean `lambda_delta` across the combinations assigned to this path.
    pub mean_lambda_delta: f64,
}

/// Distributional summary across all `φ` reconstructed paths.
#[derive(Debug, Clone, Serialize)]
pub struct CpcvSummary {
    pub mean_slippage_bps: f64,
    pub std_slippage_bps: f64,
    pub min_slippage_bps: f64,
    pub max_slippage_bps: f64,
    pub median_slippage_bps: f64,
    pub mean_lambda_delta: f64,
    pub std_lambda_delta: f64,
}

/// The full CPCV report.
#[derive(Debug, Clone, Serialize)]
pub struct CpcvReport {
    pub n_groups: usize,
    pub k_test_groups: usize,
    pub n_combinations: usize,
    pub n_paths: usize,
    pub combinations: Vec<CombinationResult>,
    pub paths: Vec<PathResult>,
    pub summary: CpcvSummary,
}

/// Orchestrates a full Combinatorial Purged Cross-Validation run.
pub struct CpcvEvaluator {
    sim_config: SimulationConfig,
    cpcv_config: CpcvConfig,
}

impl CpcvEvaluator {
    pub fn new(sim_config: SimulationConfig, cpcv_config: CpcvConfig) -> Self {
        Self {
            sim_config,
            cpcv_config,
        }
    }

    /// Run CPCV over `events`, fitting the GMM on the purged & embargoed
    /// training set and replaying the bookmaker on the held-out test set
    /// for every `C(N,k)` combination, then reconstructing `φ`
    /// non-overlapping backtest paths from the results.
    pub async fn run(&self, events: &[MarketEvent], vector_store: &mut VectorStore) -> CpcvReport {
        let splitter = CpcvSplitter::new(self.cpcv_config.clone(), events.len());
        let combos = splitter.combinations();
        let n_paths = splitter.num_paths();

        info!(
            "CPCV: N={} groups, k={} test groups, {} combinations, φ={} paths",
            splitter.n_groups(),
            self.cpcv_config.k_test_groups,
            combos.len(),
            n_paths
        );

        let mut combination_results = Vec::with_capacity(combos.len());

        for (combo_idx, combo) in combos.iter().enumerate() {
            let fold = splitter.split(combo);

            let train_events: Vec<MarketEvent> =
                fold.train_indices.iter().map(|&i| events[i].clone()).collect();
            let test_events: Vec<MarketEvent> =
                fold.test_indices.iter().map(|&i| events[i].clone()).collect();

            let mut sim = MrMarketSimulation::new(self.sim_config.clone());
            let result = sim
                .run_oos(&train_events, &test_events, vector_store)
                .await;

            let market_vwap = compute_vwap(&test_events);
            let slippage = vwap_slippage(&result.fills, market_vwap);
            let real_impact = estimate_price_impact(&test_events);

            let test_features =
                extract_features(&test_events, &self.sim_config.symbol, self.sim_config.adv);
            let sim_impact = bookmaker_implied_impact(&result.fills, &test_features);
            let lambda_delta = sim_impact.lambda - real_impact.lambda;

            combination_results.push(CombinationResult {
                combination_index: combo_idx,
                test_groups: combo.clone(),
                n_train_events: train_events.len(),
                n_test_events: test_events.len(),
                vwap_slippage: slippage,
                real_price_impact: real_impact,
                simulated_price_impact: sim_impact,
                lambda_delta,
            });
        }

        let paths = self.reconstruct_paths(&splitter, &combination_results, n_paths);
        let summary = Self::summarize(&paths);

        CpcvReport {
            n_groups: splitter.n_groups(),
            k_test_groups: self.cpcv_config.k_test_groups,
            n_combinations: combos.len(),
            n_paths,
            combinations: combination_results,
            paths,
            summary,
        }
    }

    /// Reconstruct `φ` complete backtest paths from the per-combination
    /// results using `CpcvSplitter::path_assignment()`: every path
    /// aggregates exactly one combination-result per group.
    fn reconstruct_paths(
        &self,
        splitter: &CpcvSplitter,
        combination_results: &[CombinationResult],
        n_paths: usize,
    ) -> Vec<PathResult> {
        let assignment = splitter.path_assignment();
        let mut path_slippage: Vec<Vec<f64>> = vec![Vec::new(); n_paths];
        let mut path_lambda: Vec<Vec<f64>> = vec![Vec::new(); n_paths];

        for appearances in assignment.values() {
            for &(combo_idx, slot) in appearances {
                if let Some(res) = combination_results.get(combo_idx) {
                    path_slippage[slot].push(res.vwap_slippage.notional_weighted_slippage_bps);
                    path_lambda[slot].push(res.lambda_delta);
                }
            }
        }

        (0..n_paths)
            .map(|i| PathResult {
                path_index: i,
                mean_slippage_bps: mean(&path_slippage[i]),
                mean_lambda_delta: mean(&path_lambda[i]),
            })
            .collect()
    }

    fn summarize(paths: &[PathResult]) -> CpcvSummary {
        let slippages: Vec<f64> = paths.iter().map(|p| p.mean_slippage_bps).collect();
        let lambdas: Vec<f64> = paths.iter().map(|p| p.mean_lambda_delta).collect();

        let mean_slippage = mean(&slippages);
        let std_slippage = std_dev(&slippages, mean_slippage);
        let (min_slippage, max_slippage) = min_max(&slippages);
        let median_slippage = median(&slippages);

        let mean_lambda = mean(&lambdas);
        let std_lambda = std_dev(&lambdas, mean_lambda);

        CpcvSummary {
            mean_slippage_bps: mean_slippage,
            std_slippage_bps: std_slippage,
            min_slippage_bps: min_slippage,
            max_slippage_bps: max_slippage,
            median_slippage_bps: median_slippage,
            mean_lambda_delta: mean_lambda,
            std_lambda_delta: std_lambda,
        }
    }
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

fn std_dev(v: &[f64], mean_val: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let variance = v.iter().map(|x| (x - mean_val).powi(2)).sum::<f64>() / v.len() as f64;
    variance.sqrt()
}

fn min_max(v: &[f64]) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let mut min_v = v[0];
    let mut max_v = v[0];
    for &x in v {
        if x < min_v {
            min_v = x;
        }
        if x > max_v {
            max_v = x;
        }
    }
    (min_v, max_v)
}

fn median(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut sorted = v.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n % 2 == 0 {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iex::parser::generate_synthetic_events;

    #[tokio::test]
    async fn cpcv_report_has_expected_shape() {
        let events = generate_synthetic_events("AAPL", 600);

        let sim_config = SimulationConfig {
            symbol: "AAPL".to_string(),
            adv: 1_000_000.0,
            fill_probability: 0.5,
            ..Default::default()
        };
        let cpcv_config = CpcvConfig {
            n_groups: 6,
            k_test_groups: 2,
            purge_window: 5,
            embargo_pct: 0.01,
        };

        let evaluator = CpcvEvaluator::new(sim_config, cpcv_config);
        let mut store = VectorStore::in_memory();
        let report = evaluator.run(&events, &mut store).await;

        assert_eq!(report.n_groups, 6);
        assert_eq!(report.k_test_groups, 2);
        assert_eq!(report.n_combinations, 15, "C(6,2) should be 15");
        assert_eq!(report.n_paths, 5, "phi[6,2] = C(5,1) = 5");
        assert_eq!(report.combinations.len(), 15);
        assert_eq!(report.paths.len(), 5);

        for combo in &report.combinations {
            assert!(combo.n_train_events + combo.n_test_events <= events.len());
            assert!(combo.n_test_events > 0);
        }

        assert!(report.summary.mean_slippage_bps.is_finite());
        assert!(report.summary.mean_lambda_delta.is_finite());
    }
}
