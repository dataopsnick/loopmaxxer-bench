//! Mr. Market Simulation Engine
//!
//! Replays IEX historical market events through the full bookmaking pipeline:
//! 1. Load IEX historical data (PCAP/CSV) or generate synthetic events
//! 2. Extract order-flow features and store in MemoryDB vector store
//! 3. Fit 3-component GMM via EM
//! 4. Replay events through Bookmaker (reservation pricing + OFI + risk gate)
//! 5. Simulate fills when our quotes are crossed by incoming trades
//! 6. Update portfolio delta atomically via AlignedGreeksTracker
//! 7. Evaluate Whalley-Wilmott hedge bands for delta rebalancing
//! 8. After replay, run MLE position inference
//! 9. Output results: optimal q*, GMM params, P&L breakdown

use crate::bookmaker::{BookQuote, Bookmaker, BookmakerConfig};
use crate::gmm::em::{EmConfig, GmmFitter};
use crate::gmm::features::{extract_features, features_to_arrays, OrderFlowFeatures};
use crate::gmm::model::{GmmModel, TraderState};
use crate::iex::parser::{generate_synthetic_events, load_events};
use crate::iex::MarketEvent;
use crate::memorydb::vector_store::{FeatureVector, VectorStore};
use crate::mle::likelihood::LikelihoodParams;
use crate::mle::position::{MlePositionInferer, MlePositionResult};
use crate::portfolio::AlignedGreeksTracker;
use crate::sofr::{AssetHedgeParameters, SOFRHedgeController};
use crate::symbology::{sources, PackedAssetKey};
use serde::Serialize;
use tracing::{info, warn};

/// Configuration for the Mr. Market simulation.
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    /// Symbol to simulate (e.g. "AAPL")
    pub symbol: String,
    /// Average daily volume (shares) for feature normalization
    pub adv: f64,
    /// SOFR base rate (e.g. 0.0535 for 5.35%)
    pub sofr_rate: f64,
    /// Risk aversion parameter γ
    pub risk_aversion: f64,
    /// Margin haircut
    pub margin_haircut: f64,
    /// Borrow premium
    pub borrow_premium: f64,
    /// Liquidity parameter κ
    pub liquidity_kappa: f64,
    /// OFI EWMA decay
    pub ofi_decay: f64,
    /// OFI multiplier
    pub ofi_multiplier: f64,
    /// Time to horizon (fraction of day)
    pub time_to_horizon: f64,
    /// Max order quantity (risk gate)
    pub max_order_qty: u32,
    /// Max price (risk gate)
    pub max_price_usd: f64,
    /// Max absolute delta (risk gate)
    pub max_delta: f64,
    /// MLE grid search bounds
    pub q_min: f64,
    pub q_max: f64,
    /// MLE grid resolution
    pub n_grid: usize,
    /// EM config
    pub em_config: EmConfig,
    /// Initial cash balance
    pub initial_cash: f64,
    /// Fill probability when our quote is crossed (0..1)
    pub fill_probability: f64,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            symbol: "AAPL".to_string(),
            adv: 10_000_000.0,
            sofr_rate: 0.0535,
            risk_aversion: 0.015,
            margin_haircut: 0.15,
            borrow_premium: 0.0025,
            liquidity_kappa: 2.1,
            ofi_decay: 0.95,
            ofi_multiplier: 0.001,
            time_to_horizon: 0.45,
            max_order_qty: 1000,
            max_price_usd: 5000.0,
            max_delta: 5000.0,
            q_min: -5000.0,
            q_max: 5000.0,
            n_grid: 200,
            em_config: EmConfig::default(),
            initial_cash: 100_000_000.0,
            fill_probability: 0.3,
        }
    }
}

/// Per-fill record for P&L attribution.
#[derive(Debug, Clone, Serialize)]
pub struct FillRecord {
    pub timestamp_ns: u64,
    pub side: String,
    pub price: f64,
    pub size: f64,
    pub pnl: f64,
}

/// Summary P&L breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct PnlBreakdown {
    pub spread_revenue: f64,
    pub adverse_selection_cost: f64,
    pub sofr_carry_cost: f64,
    pub hedging_cost: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub total_pnl: f64,
    pub n_fills: usize,
    pub n_quotes: usize,
    pub n_hedges: usize,
    pub n_rejections: usize,
    pub final_delta: f64,
    pub final_cash: f64,
}

/// Complete simulation result.
#[derive(Debug, Clone, Serialize)]
pub struct SimulationResult {
    pub symbol: String,
    pub n_events: usize,
    pub n_features: usize,
    pub gmm: GmmModel,
    pub mle_result: MlePositionResult,
    pub pnl: PnlBreakdown,
    pub fills: Vec<FillRecord>,
    pub final_quote: Option<BookQuote>,
}

/// The Mr. Market simulation engine.
pub struct MrMarketSimulation {
    config: SimulationConfig,
    bookmaker: Bookmaker,
    greeks: AlignedGreeksTracker,
    sofr_controller: SOFRHedgeController,
    feature_extractor: OrderFlowFeatures,
    asset_key: PackedAssetKey,
}

impl MrMarketSimulation {
    /// Create a new simulation with the given configuration.
    pub fn new(config: SimulationConfig) -> Self {
        let bm_config = BookmakerConfig {
            risk_aversion_gamma: config.risk_aversion,
            sofr_base_rate: config.sofr_rate,
            margin_haircut: config.margin_haircut,
            borrow_premium: config.borrow_premium,
            liquidity_kappa: config.liquidity_kappa,
            ofi_decay: config.ofi_decay,
            ofi_multiplier: config.ofi_multiplier,
            time_to_horizon: config.time_to_horizon,
            max_order_qty: config.max_order_qty,
            max_price_usd: config.max_price_usd,
            max_delta: config.max_delta,
        };

        let asset_key = PackedAssetKey::new_equity(sources::NMS, &config.symbol);

        Self {
            bookmaker: Bookmaker::new(bm_config),
            greeks: AlignedGreeksTracker::new(config.initial_cash),
            sofr_controller: SOFRHedgeController::new(config.risk_aversion, config.sofr_rate),
            feature_extractor: OrderFlowFeatures::new(&config.symbol, config.adv),
            asset_key,
            config,
        }
    }

    /// Run the full simulation pipeline on a set of market events.
    pub async fn run(
        &mut self,
        events: &[MarketEvent],
        vector_store: &mut VectorStore,
    ) -> SimulationResult {
        info!(
            "Starting Mr. Market simulation for {} with {} events",
            self.config.symbol,
            events.len()
        );

        // ── Phase 1: Feature extraction & storage ──────────────────────
        let features = extract_features(events, &self.config.symbol, self.config.adv);
        info!("Extracted {} feature vectors", features.len());

        for fv in &features {
            if let Err(e) = vector_store.store(fv).await {
                warn!("Failed to store feature vector: {}", e);
            }
        }

        // ── Phase 2: GMM fitting via EM ────────────────────────────────
        let data = features_to_arrays(&features);
        let fitter = GmmFitter::new(self.config.em_config.clone());
        let gmm = fitter.fit(&data, 6);

        info!(
            "GMM fitted: π_noise={:.4} π_inst={:.4} π_informed={:.4} ll={:.4}",
            gmm.pi_noise(),
            gmm.pi_institutional(),
            gmm.pi_informed(),
            gmm.log_likelihood
        );

        // Store GMM params in vector store
        if let Ok(json) = gmm.to_json() {
            if let Err(e) = vector_store.store_gmm(&self.config.symbol, &json).await {
                warn!("Failed to store GMM params: {}", e);
            }
        }

        // ── Phase 3: Event replay through bookmaking engine ───────────
        let mut fills = Vec::new();
        let mut current_quote: Option<BookQuote> = None;
        let mut n_quotes = 0usize;
        let mut n_rejections = 0usize;
        let mut n_hedges = 0usize;
        let mut spread_revenue = 0.0f64;
        let mut adverse_selection_cost = 0.0f64;
        let mut hedging_cost = 0.0f64;
        let mut last_mid = 0.0f64;

        for event in events {
            // Feed every event through the feature extractor to maintain state
            let trade_fv = self.feature_extractor.process_event(event);

            match event {
                MarketEvent::QuoteUpdate {
                    symbol,
                    bid_price,
                    bid_size,
                    ask_price,
                    ask_size,
                    timestamp_ns,
                } => {
                    if symbol != &self.config.symbol {
                        continue;
                    }

                    let mid = (bid_price + ask_price) / 2.0;
                    last_mid = mid;

                    let position = self.greeks.load_delta();

                    // Compute volatility from feature extractor's price history
                    let vol = self.feature_extractor.current_realized_vol();

                    // Generate quote through bookmaker
                    if let Some(quote) = self.bookmaker.compute_quote(
                        self.asset_key,
                        mid,
                        *bid_price,
                        *bid_size,
                        *ask_price,
                        *ask_size,
                        position,
                        vol,
                        *timestamp_ns,
                    ) {
                        n_quotes += 1;
                        current_quote = Some(quote);
                    } else {
                        n_rejections += 1;
                    }
                }

                MarketEvent::Trade {
                    symbol,
                    price,
                    size,
                    timestamp_ns,
                } => {
                    if symbol != &self.config.symbol {
                        continue;
                    }

                    // Check if our quote was crossed → simulate fill
                    if let Some(ref quote) = current_quote {
                        let is_buy_fill = *price >= quote.ask_price;
                        let is_sell_fill = *price <= quote.bid_price;

                        if is_buy_fill || is_sell_fill {
                            // Probabilistic fill model
                            let should_fill = self.config.fill_probability > 0.0
                                && simple_random(self.config.fill_probability);

                            if should_fill {
                                let fill_size =
                                    size.min(self.config.max_order_qty as f64);
                                let fill_price = if is_buy_fill {
                                    quote.ask_price
                                } else {
                                    quote.bid_price
                                };

                                // Update delta: we sell at ask (short delta) or buy at bid (long delta)
                                let delta_change = if is_buy_fill {
                                    -fill_size
                                } else {
                                    fill_size
                                };

                                self.greeks.add_delta(delta_change);

                                // P&L: spread revenue minus adverse selection
                                let mid = if last_mid > 0.0 { last_mid } else { *price };
                                let spread_pnl = if is_buy_fill {
                                    (fill_price - mid) * fill_size
                                } else {
                                    (mid - fill_price) * fill_size
                                };

                                // Classify the trade using GMM
                                let fv = if let Some(ref tfv) = trade_fv {
                                    tfv.clone()
                                } else {
                                    FeatureVector {
                                        timestamp_ns: *timestamp_ns,
                                        symbol: self.config.symbol.clone(),
                                        normalized_trade_size: *size / self.config.adv,
                                        signed_order_flow: if is_buy_fill {
                                            -*size
                                        } else {
                                            *size
                                        },
                                        ofi_ewma: self.feature_extractor.current_ofi(),
                                        spread_width: if last_mid > 0.0 {
                                            quote.spread_width
                                        } else {
                                            0.02
                                        },
                                        vol_atm: self.feature_extractor.current_realized_vol(),
                                        return_predictability: 0.0,
                                    }
                                };

                                let trader_state = gmm.classify(&fv.to_array());
                                let adverse = match trader_state {
                                    TraderState::Informed => {
                                        (mid - *price).abs() * fill_size * 0.5
                                    }
                                    _ => 0.0,
                                };

                                spread_revenue += spread_pnl;
                                adverse_selection_cost += adverse;

                                // Update cash
                                self.greeks.add_cash(spread_pnl - adverse);

                                fills.push(FillRecord {
                                    timestamp_ns: *timestamp_ns,
                                    side: if is_buy_fill {
                                        "SELL".to_string()
                                    } else {
                                        "BUY".to_string()
                                    },
                                    price: fill_price,
                                    size: fill_size,
                                    pnl: spread_pnl - adverse,
                                });
                            }
                        }
                    }

                    // ── Whalley-Wilmott hedge band evaluation ──────────
                    let current_delta = self.greeks.load_delta();
                    let hedge_params = AssetHedgeParameters {
                        volatility: self.feature_extractor.current_realized_vol(),
                        gamma: 0.04,
                        theta: -0.02,
                        half_spread: 0.01,
                        sofr_borrow_premium: self.config.borrow_premium,
                    };

                    let time_to_midnight = self.config.time_to_horizon;

                    if let Some(hedge_qty) = self.sofr_controller.evaluate_delta_hedge(
                        current_delta,
                        0.0, // target delta = 0 (delta-neutral)
                        &hedge_params,
                        last_mid,
                        time_to_midnight,
                    ) {
                        // Execute hedge
                        let hedge_slippage = hedge_qty.abs() * hedge_params.half_spread;
                        hedging_cost += hedge_slippage;
                        self.greeks.add_delta(hedge_qty);
                        self.greeks.add_cash(-hedge_slippage);
                        n_hedges += 1;
                    }
                }

                MarketEvent::PriceLevelUpdate { .. } => {}
            }
        }

        // ── Phase 4: MLE position inference ────────────────────────────
        let ll_params = LikelihoodParams {
            spot_price: if last_mid > 0.0 { last_mid } else { 150.0 },
            sofr_rate: self.config.sofr_rate,
            margin_haircut: self.config.margin_haircut,
            borrow_premium: self.config.borrow_premium,
            liquidity_kappa: self.config.liquidity_kappa,
            risk_aversion_gamma: self.config.risk_aversion,
            time_to_horizon: self.config.time_to_horizon,
            half_spread: 0.01,
        };

        let inferer = MlePositionInferer::new(
            ll_params,
            self.config.q_min,
            self.config.q_max,
            self.config.n_grid,
        );

        let mle_result = inferer.infer(&gmm, &features);

        info!(
            "MLE inference complete: optimal_q={:.2}, max_ll={:.4}",
            mle_result.optimal_q, mle_result.max_log_likelihood
        );

        // ── Phase 5: Compute final P&L ─────────────────────────────────
        let final_delta = self.greeks.load_delta();
        let final_cash = self.greeks.load_cash();
        let unrealized_pnl = final_delta * last_mid;

        // SOFR carry cost for the remaining position
        let sofr_carry_cost = self.sofr_controller.sofr_financing_cost(
            final_delta,
            last_mid,
            self.config.margin_haircut,
            self.config.borrow_premium,
        ) * self.config.time_to_horizon;

        let realized_pnl = spread_revenue - adverse_selection_cost - hedging_cost;
        let total_pnl = realized_pnl + unrealized_pnl - sofr_carry_cost;

        let pnl = PnlBreakdown {
            spread_revenue,
            adverse_selection_cost,
            sofr_carry_cost,
            hedging_cost,
            realized_pnl,
            unrealized_pnl,
            total_pnl,
            n_fills: fills.len(),
            n_quotes,
            n_hedges,
            n_rejections,
            final_delta,
            final_cash,
        };

        info!(
            "Simulation complete: {} fills, {} quotes, {} hedges, {} rejections, total P&L={:.2}",
            pnl.n_fills, pnl.n_quotes, pnl.n_hedges, pnl.n_rejections, pnl.total_pnl
        );

        SimulationResult {
            symbol: self.config.symbol.clone(),
            n_events: events.len(),
            n_features: features.len(),
            gmm,
            mle_result,
            pnl,
            fills,
            final_quote: current_quote,
        }
    }

    /// Run simulation with synthetic data (for testing without IEX files).
    pub async fn run_synthetic(
        &mut self,
        n_events: usize,
        vector_store: &mut VectorStore,
    ) -> SimulationResult {
        let events = generate_synthetic_events(&self.config.symbol, n_events);
        self.run(&events, vector_store).await
    }

    /// Run simulation by loading events from a file (PCAP or CSV).
    pub async fn run_from_file(
        &mut self,
        path: &str,
        vector_store: &mut VectorStore,
    ) -> Result<SimulationResult, String> {
        let events = load_events(path)?;
        Ok(self.run(&events, vector_store).await)
    }
}

/// Simple deterministic pseudo-random fill decision.
/// Uses a thread-local LCG to decide whether a crossed quote results in a fill.
fn simple_random(probability: f64) -> bool {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = Cell::new(0x123456789ABCDEF0);
    }

    STATE.with(|s| {
        let mut v = s.get();
        v = v.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        s.set(v);
        let rand_val = ((v >> 11) as f64) / (u64::MAX as f64);
        rand_val < probability
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_synthetic_simulation() {
        let config = SimulationConfig {
            symbol: "AAPL".to_string(),
            adv: 1_000_000.0,
            fill_probability: 0.5,
            ..Default::default()
        };

        let mut sim = MrMarketSimulation::new(config);
        let mut store = VectorStore::in_memory();

        let result = sim.run_synthetic(500, &mut store).await;

        assert_eq!(result.symbol, "AAPL");
        assert!(result.n_events >= 500);
        assert!(result.n_features > 0, "Should extract features from trades");
        assert!(result.pnl.n_quotes > 0, "Should generate quotes");
        assert!(result.mle_result.optimal_q.is_finite());
        assert!(result.mle_result.max_log_likelihood.is_finite());
    }

    #[tokio::test]
    async fn gmm_fits_and_classifies() {
        let config = SimulationConfig::default();
        let mut sim = MrMarketSimulation::new(config);
        let mut store = VectorStore::in_memory();

        let result = sim.run_synthetic(1000, &mut store).await;

        let total_weight: f64 = result.gmm.components.iter().map(|c| c.weight).sum();
        assert!(
            (total_weight - 1.0).abs() < 0.2,
            "GMM weights should sum to ~1: {}",
            total_weight
        );
    }
}