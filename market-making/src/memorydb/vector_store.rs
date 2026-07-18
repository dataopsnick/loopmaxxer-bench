//! Vector Store for Time-Series Feature Similarity
//!
//! Stores market microstructure feature vectors in MemoryDB and
//! provides similarity queries for MLE position inference.

use super::client::InMemoryStore;
use serde::{Deserialize, Serialize};

/// A market microstructure feature vector.
///
/// Features extracted from order flow for GMM fitting and MLE inference:
/// - `normalized_trade_size`: Trade size relative to ADV
/// - `signed_order_flow`: Net signed order flow (buy - sell)
/// - `ofi_ewma`: EWMA of order flow imbalance
/// - `spread_width`: Current bid-ask spread
/// - `vol_atm`: ATM implied volatility
/// - `return_predictability`: Short-term return predictability score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    pub timestamp_ns: u64,
    pub symbol: String,
    pub normalized_trade_size: f64,
    pub signed_order_flow: f64,
    pub ofi_ewma: f64,
    pub spread_width: f64,
    pub vol_atm: f64,
    pub return_predictability: f64,
}

impl FeatureVector {
    /// Convert to a flat array for GMM fitting.
    pub fn to_array(&self) -> [f64; 6] {
        [
            self.normalized_trade_size,
            self.signed_order_flow,
            self.ofi_ewma,
            self.spread_width,
            self.vol_atm,
            self.return_predictability,
        ]
    }

    /// Convert to a Vec<f64> for storage.
    pub fn to_vec(&self) -> Vec<f64> {
        vec![
            self.normalized_trade_size,
            self.signed_order_flow,
            self.ofi_ewma,
            self.spread_width,
            self.vol_atm,
            self.return_predictability,
        ]
    }

    /// Create from a flat array.
    pub fn from_array(arr: &[f64], symbol: &str, timestamp_ns: u64) -> Self {
        Self {
            timestamp_ns,
            symbol: symbol.to_string(),
            normalized_trade_size: arr.get(0).copied().unwrap_or(0.0),
            signed_order_flow: arr.get(1).copied().unwrap_or(0.0),
            ofi_ewma: arr.get(2).copied().unwrap_or(0.0),
            spread_width: arr.get(3).copied().unwrap_or(0.0),
            vol_atm: arr.get(4).copied().unwrap_or(0.0),
            return_predictability: arr.get(5).copied().unwrap_or(0.0),
        }
    }
}

/// Vector store that manages feature vectors in MemoryDB or in-memory fallback.
pub enum VectorStore {
    /// Live MemoryDB-backed store
    MemoryDb(super::client::MemoryDbClient),
    /// In-memory fallback for development
    InMemory(InMemoryStore),
}

impl VectorStore {
    /// Create a MemoryDB-backed vector store.
    pub fn memorydb(config: super::MemoryDbConfig) -> Self {
        Self::MemoryDb(super::client::MemoryDbClient::new(config))
    }

    /// Create an in-memory vector store.
    pub fn in_memory() -> Self {
        Self::InMemory(InMemoryStore::new())
    }

    /// Store a feature vector.
    pub async fn store(&mut self, feature: &FeatureVector) -> Result<(), String> {
        match self {
            Self::MemoryDb(client) => {
                client
                    .store_feature(&feature.symbol, feature.timestamp_ns, &feature.to_vec())
                    .await
            }
            Self::InMemory(store) => {
                store.store_feature(&feature.symbol, feature.timestamp_ns, feature.to_vec());
                Ok(())
            }
        }
    }

    /// Query features in a time range.
    pub async fn query_range(
        &mut self,
        symbol: &str,
        start_ns: u64,
        end_ns: u64,
    ) -> Result<Vec<FeatureVector>, String> {
        match self {
            Self::MemoryDb(client) => {
                let results = client.query_time_range(symbol, start_ns, end_ns).await?;
                Ok(results
                    .into_iter()
                    .map(|(ts, vec)| FeatureVector::from_array(&vec, symbol, ts))
                    .collect())
            }
            Self::InMemory(store) => {
                let results = store.query_time_range(symbol, start_ns, end_ns);
                Ok(results
                    .into_iter()
                    .map(|(ts, vec)| FeatureVector::from_array(&vec, symbol, ts))
                    .collect())
            }
        }
    }

    /// Find the K most similar historical feature vectors using Euclidean distance.
    pub fn find_similar(&self, target: &FeatureVector, k: usize) -> Vec<(f64, FeatureVector)> {
        match self {
            Self::InMemory(store) => {
                let all_features: Vec<&(u64, Vec<f64>)> = store
                    .timeline
                    .get(&target.symbol)
                    .map(|v| v.iter().collect())
                    .unwrap_or_default();

                let target_arr = target.to_array();
                let mut distances: Vec<(f64, FeatureVector)> = all_features
                    .into_iter()
                    .map(|(ts, vec)| {
                        let dist = euclidean_distance(&target_arr, vec);
                        let fv = FeatureVector::from_array(vec, &target.symbol, *ts);
                        (dist, fv)
                    })
                    .collect();

                distances
                    .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                distances.truncate(k);
                distances
            }
            Self::MemoryDb(_) => {
                // For MemoryDB, similarity search would use RediSearch FT.SEARCH
                Vec::new()
            }
        }
    }

    /// Store GMM parameters.
    pub async fn store_gmm(&mut self, symbol: &str, params_json: &str) -> Result<(), String> {
        match self {
            Self::MemoryDb(client) => client.store_gmm_params(symbol, params_json).await,
            Self::InMemory(store) => {
                store.store_gmm_params(symbol, params_json);
                Ok(())
            }
        }
    }

    /// Connect to MemoryDB (no-op for in-memory).
    pub async fn connect(&mut self) -> Result<(), String> {
        match self {
            Self::MemoryDb(client) => client.connect().await,
            Self::InMemory(_) => Ok(()),
        }
    }
}

/// Compute Euclidean distance between two feature vectors.
#[inline(always)]
fn euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    let mut sum = 0.0;
    for i in 0..n {
        let diff = a[i] - b[i];
        sum += diff * diff;
    }
    sum.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_vector_roundtrip() {
        let fv = FeatureVector {
            timestamp_ns: 1000,
            symbol: "AAPL".to_string(),
            normalized_trade_size: 0.5,
            signed_order_flow: 100.0,
            ofi_ewma: 0.001,
            spread_width: 0.02,
            vol_atm: 0.20,
            return_predictability: 0.3,
        };

        let arr = fv.to_array();
        let fv2 = FeatureVector::from_array(&arr, "AAPL", 1000);
        assert!((fv.normalized_trade_size - fv2.normalized_trade_size).abs() < 1e-9);
        assert!((fv.signed_order_flow - fv2.signed_order_flow).abs() < 1e-9);
    }

    #[test]
    fn in_memory_store_and_query() {
        let mut store = VectorStore::in_memory();
        let fv = FeatureVector {
            timestamp_ns: 1000,
            symbol: "AAPL".to_string(),
            normalized_trade_size: 0.5,
            signed_order_flow: 100.0,
            ofi_ewma: 0.001,
            spread_width: 0.02,
            vol_atm: 0.20,
            return_predictability: 0.3,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            store.store(&fv).await.unwrap();
            let results = store.query_range("AAPL", 0, 2000).await.unwrap();
            assert_eq!(results.len(), 1);
            assert!((results[0].normalized_trade_size - 0.5).abs() < 1e-9);
        });
    }
}