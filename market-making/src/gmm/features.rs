//! Order Flow Feature Extraction
//!
//! Extracts microstructure features from IEX market events for GMM fitting
//! and MLE position inference.

use crate::iex::MarketEvent;
use crate::memorydb::vector_store::FeatureVector;

/// Accumulator for order-flow features extracted from market events.
pub struct OrderFlowFeatures {
    symbol: String,
    adv: f64,
    ofi_ewma: f64,
    ofi_decay: f64,
    signed_flow_accumulator: f64,
    trade_count: u64,
    last_mid_price: f64,
    last_spread: f64,
    price_history: Vec<f64>,
    max_history: usize,
}

impl OrderFlowFeatures {
    pub fn new(symbol: &str, adv: f64) -> Self {
        Self {
            symbol: symbol.to_string(),
            adv: adv.max(1.0),
            ofi_ewma: 0.0,
            ofi_decay: 0.95,
            signed_flow_accumulator: 0.0,
            trade_count: 0,
            last_mid_price: 0.0,
            last_spread: 0.0,
            price_history: Vec::with_capacity(100),
            max_history: 100,
        }
    }

    /// Process a market event and optionally produce a feature vector.
    pub fn process_event(&mut self, event: &MarketEvent) -> Option<FeatureVector> {
        match event {
            MarketEvent::QuoteUpdate {
                symbol,
                bid_price,
                bid_size,
                ask_price,
                ask_size,
                timestamp_ns: _,
            } => {
                if symbol != &self.symbol {
                    return None;
                }

                let mid = (bid_price + ask_price) / 2.0;
                let spread = ask_price - bid_price;
                self.last_mid_price = mid;
                self.last_spread = spread;

                let ofi = bid_size - ask_size;
                self.ofi_ewma = self.ofi_decay * self.ofi_ewma + (1.0 - self.ofi_decay) * ofi;

                None
            }
            MarketEvent::Trade {
                symbol,
                price,
                size,
                timestamp_ns,
            } => {
                if symbol != &self.symbol {
                    return None;
                }

                self.trade_count += 1;

                let is_buy = if self.last_mid_price > 0.0 {
                    *price >= self.last_mid_price
                } else {
                    true
                };

                let signed_flow = if is_buy { *size } else { -*size };
                self.signed_flow_accumulator += signed_flow;

                self.price_history.push(*price);
                if self.price_history.len() > self.max_history {
                    self.price_history.remove(0);
                }

                let return_pred = self.compute_return_predictability();
                let vol_atm = self.compute_realized_vol();
                let normalized_size = *size / self.adv;

                Some(FeatureVector {
                    timestamp_ns: *timestamp_ns,
                    symbol: self.symbol.clone(),
                    normalized_trade_size: normalized_size,
                    signed_order_flow: signed_flow,
                    ofi_ewma: self.ofi_ewma,
                    spread_width: self.last_spread,
                    vol_atm,
                    return_predictability: return_pred,
                })
            }
            MarketEvent::PriceLevelUpdate { .. } => None,
        }
    }

    fn compute_return_predictability(&self) -> f64 {
        if self.price_history.len() < 5 {
            return 0.0;
        }

        let returns: Vec<f64> = self
            .price_history
            .windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect();

        if returns.len() < 2 {
            return 0.0;
        }

        let mean: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance: f64 =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;

        if variance < 1e-12 {
            return 0.0;
        }

        let n = returns.len();
        let cov: f64 = (0..n - 1)
            .map(|i| (returns[i] - mean) * (returns[i + 1] - mean))
            .sum::<f64>()
            / (n - 1) as f64;

        cov / variance
    }

    fn compute_realized_vol(&self) -> f64 {
        if self.price_history.len() < 3 {
            return 0.20;
        }

        let returns: Vec<f64> = self
            .price_history
            .windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect();

        let mean: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance: f64 =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;

        variance.sqrt()
    }

    pub fn current_ofi(&self) -> f64 {
        self.ofi_ewma
    }

    /// Get the current realized volatility from the price history.
    pub fn current_realized_vol(&self) -> f64 {
        self.compute_realized_vol()
    }

    pub fn current_mid(&self) -> f64 {
        self.last_mid_price
    }

    pub fn current_spread(&self) -> f64 {
        self.last_spread
    }

    pub fn accumulated_signed_flow(&self) -> f64 {
        self.signed_flow_accumulator
    }

    pub fn trade_count(&self) -> u64 {
        self.trade_count
    }
}

/// Extract all feature vectors from a sequence of market events.
pub fn extract_features(events: &[MarketEvent], symbol: &str, adv: f64) -> Vec<FeatureVector> {
    let mut extractor = OrderFlowFeatures::new(symbol, adv);
    let mut features = Vec::new();

    for event in events {
        if let Some(fv) = extractor.process_event(event) {
            features.push(fv);
        }
    }

    features
}

/// Convert feature vectors to flat arrays for GMM fitting.
pub fn features_to_arrays(features: &[FeatureVector]) -> Vec<Vec<f64>> {
    features.iter().map(|f| f.to_vec()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_features_from_synthetic() {
        let events = crate::iex::parser::generate_synthetic_events("AAPL", 200);
        let features = extract_features(&events, "AAPL", 10000.0);
        assert!(!features.is_empty(), "Should extract features from trades");
    }
}