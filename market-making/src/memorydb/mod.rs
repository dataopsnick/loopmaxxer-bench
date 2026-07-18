//! AWS MemoryDB Module
//!
//! Redis-compatible in-memory data store for time-series feature vectors
//! and GMM model parameter caching. Falls back to in-memory store for
//! local development when MemoryDB is not available.

pub mod client;
pub mod vector_store;

/// Configuration for AWS MemoryDB (Redis-compatible) connection.
#[derive(Debug, Clone)]
pub struct MemoryDbConfig {
    /// MemoryDB cluster endpoint (e.g. "clustercfg.memorydb-cluster.xxxxxx.memorydb.us-east-1.amazonaws.com")
    pub endpoint: String,
    /// Redis port (default 6379)
    pub port: u16,
    /// Use TLS (rediss://) — recommended for MemoryDB
    pub use_tls: bool,
    /// Optional AUTH token for MemoryDB cluster
    pub auth_token: Option<String>,
    /// Key prefix for namespacing (e.g. "mrmarket")
    pub key_prefix: String,
}

impl Default for MemoryDbConfig {
    fn default() -> Self {
        Self {
            endpoint: "127.0.0.1".to_string(),
            port: 6379,
            use_tls: false,
            auth_token: None,
            key_prefix: "mrmarket".to_string(),
        }
    }
}