//! MemoryDB Client
//!
//! Redis-protocol client for AWS MemoryDB with TLS support.
//! Falls back to local Redis when MemoryDB is not available.

use super::MemoryDbConfig;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use tracing::info;

/// MemoryDB / Redis client for time-series feature storage.
pub struct MemoryDbClient {
    config: MemoryDbConfig,
    connection: Option<ConnectionManager>,
}

impl MemoryDbClient {
    /// Create a new MemoryDB client (does not connect yet).
    pub fn new(config: MemoryDbConfig) -> Self {
        Self {
            config,
            connection: None,
        }
    }

    /// Build the Redis connection URL.
    fn build_url(&self) -> String {
        let scheme = if self.config.use_tls { "rediss" } else { "redis" };
        let auth = match &self.config.auth_token {
            Some(token) => format!(":{}@", token),
            None => String::new(),
        };
        format!(
            "{}://{}{}:{}",
            scheme, auth, self.config.endpoint, self.config.port
        )
    }

    /// Connect to MemoryDB asynchronously.
    pub async fn connect(&mut self) -> Result<(), String> {
        let url = self.build_url();
        info!("Connecting to MemoryDB: {}", self.config.endpoint);

        let client =
            redis::Client::open(url).map_err(|e| format!("Failed to create Redis client: {}", e))?;

        let conn = ConnectionManager::new(client)
            .await
            .map_err(|e| format!("Failed to connect to MemoryDB: {}", e))?;

        self.connection = Some(conn);
        info!("MemoryDB connection established");
        Ok(())
    }

    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    /// Store a feature vector at a given timestamp key.
    pub async fn store_feature(
        &mut self,
        symbol: &str,
        timestamp_ns: u64,
        features: &[f64],
    ) -> Result<(), String> {
        let conn = self
            .connection
            .as_mut()
            .ok_or("Not connected to MemoryDB")?;

        let key = format!(
            "{}:features:{}:{}",
            self.config.key_prefix, symbol, timestamp_ns
        );
        let feature_str: Vec<String> = features.iter().map(|f| f.to_string()).collect();
        let value = feature_str.join(",");

        let _: () = conn
            .hset(&key, "vector", &value)
            .await
            .map_err(|e| format!("Redis HSET failed: {}", e))?;

        let _: () = conn
            .hset(&key, "timestamp", timestamp_ns)
            .await
            .map_err(|e| format!("Redis HSET failed: {}", e))?;

        // Add to sorted set for time-range queries
        let zset_key = format!("{}:timeline:{}", self.config.key_prefix, symbol);
        let _: () = conn
            .zadd(&zset_key, &key, timestamp_ns as f64)
            .await
            .map_err(|e| format!("Redis ZADD failed: {}", e))?;

        Ok(())
    }

    /// Retrieve features for a specific timestamp.
    pub async fn get_feature(
        &mut self,
        symbol: &str,
        timestamp_ns: u64,
    ) -> Result<Vec<f64>, String> {
        let conn = self
            .connection
            .as_mut()
            .ok_or("Not connected to MemoryDB")?;

        let key = format!(
            "{}:features:{}:{}",
            self.config.key_prefix, symbol, timestamp_ns
        );
        let value: String = conn
            .hget(&key, "vector")
            .await
            .map_err(|e| format!("Redis HGET failed: {}", e))?;

        let features: Vec<f64> = value
            .split(',')
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .collect();

        Ok(features)
    }

    /// Query features in a time range [start_ns, end_ns).
    pub async fn query_time_range(
        &mut self,
        symbol: &str,
        start_ns: u64,
        end_ns: u64,
    ) -> Result<Vec<(u64, Vec<f64>)>, String> {
        let conn = self
            .connection
            .as_mut()
            .ok_or("Not connected to MemoryDB")?;

        let zset_key = format!("{}:timeline:{}", self.config.key_prefix, symbol);
        let keys: Vec<String> = conn
            .zrangebyscore(&zset_key, start_ns as f64, end_ns as f64)
            .await
            .map_err(|e| format!("Redis ZRANGEBYSCORE failed: {}", e))?;

        let mut results = Vec::new();
        for key in keys {
            let ts: u64 = conn
                .hget(&key, "timestamp")
                .await
                .map_err(|e| format!("Redis HGET failed: {}", e))?;
            let vec_str: String = conn
                .hget(&key, "vector")
                .await
                .map_err(|e| format!("Redis HGET failed: {}", e))?;
            let features: Vec<f64> = vec_str
                .split(',')
                .filter_map(|s| s.trim().parse::<f64>().ok())
                .collect();
            results.push((ts, features));
        }

        Ok(results)
    }

    /// Store GMM model parameters.
    pub async fn store_gmm_params(
        &mut self,
        symbol: &str,
        params_json: &str,
    ) -> Result<(), String> {
        let conn = self
            .connection
            .as_mut()
            .ok_or("Not connected to MemoryDB")?;

        let key = format!("{}:gmm:{}", self.config.key_prefix, symbol);
        let _: () = conn
            .set(&key, params_json)
            .await
            .map_err(|e| format!("Redis SET failed: {}", e))?;

        Ok(())
    }

    /// Retrieve GMM model parameters.
    pub async fn get_gmm_params(&mut self, symbol: &str) -> Result<String, String> {
        let conn = self
            .connection
            .as_mut()
            .ok_or("Not connected to MemoryDB")?;

        let key = format!("{}:gmm:{}", self.config.key_prefix, symbol);
        let value: String = conn
            .get(&key)
            .await
            .map_err(|e| format!("Redis GET failed: {}", e))?;

        Ok(value)
    }

    /// Ping the MemoryDB to check connectivity.
    pub async fn ping(&mut self) -> Result<(), String> {
        let conn = self
            .connection
            .as_mut()
            .ok_or("Not connected to MemoryDB")?;

        let _: String = redis::cmd("PING")
            .query_async(conn)
            .await
            .map_err(|e| format!("Redis PING failed: {}", e))?;

        Ok(())
    }
}

/// In-memory fallback store for when MemoryDB is not available.
pub struct InMemoryStore {
    pub features: std::collections::HashMap<(String, u64), Vec<f64>>,
    pub timeline: std::collections::HashMap<String, Vec<(u64, Vec<f64>)>>,
    pub gmm_params: std::collections::HashMap<String, String>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            features: std::collections::HashMap::new(),
            timeline: std::collections::HashMap::new(),
            gmm_params: std::collections::HashMap::new(),
        }
    }

    pub fn store_feature(&mut self, symbol: &str, timestamp_ns: u64, features: Vec<f64>) {
        self.features
            .insert((symbol.to_string(), timestamp_ns), features.clone());
        self.timeline
            .entry(symbol.to_string())
            .or_default()
            .push((timestamp_ns, features));
    }

    pub fn get_feature(&self, symbol: &str, timestamp_ns: u64) -> Option<&Vec<f64>> {
        self.features.get(&(symbol.to_string(), timestamp_ns))
    }

    pub fn query_time_range(
        &self,
        symbol: &str,
        start_ns: u64,
        end_ns: u64,
    ) -> Vec<(u64, Vec<f64>)> {
        self.timeline
            .get(symbol)
            .map(|v| {
                v.iter()
                    .filter(|(ts, _)| *ts >= start_ns && *ts < end_ns)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn store_gmm_params(&mut self, symbol: &str, params_json: &str) {
        self.gmm_params
            .insert(symbol.to_string(), params_json.to_string());
    }

    pub fn get_gmm_params(&self, symbol: &str) -> Option<&String> {
        self.gmm_params.get(symbol)
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}