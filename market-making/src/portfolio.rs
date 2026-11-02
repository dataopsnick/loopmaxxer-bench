//! Atomic Portfolio Risk State (Spec §5, §15, §20)
//!
//! Lock-free, cache-line-aligned atomic containers for real-time
//! portfolio Greek exposure and SOFR cash balance tracking.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use crate::symbology::{sources, PackedAssetKey};

/// f64 data stored as AtomicU64 bits for lock-free updates.
pub struct AtomicFloat {
    bits: AtomicU64,
}

impl AtomicFloat {
    #[inline(always)]
    pub fn new(val: f64) -> Self {
        Self {
            bits: AtomicU64::new(val.to_bits()),
        }
    }

    #[inline(always)]
    pub fn load(&self) -> f64 {
        f64::from_bits(self.bits.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub fn store(&self, val: f64) {
        self.bits.store(val.to_bits(), Ordering::Release);
    }

    /// CAS-based lock-free floating-point add.
    #[inline(always)]
    pub fn fetch_add(&self, delta: f64) {
        let mut current_bits = self.bits.load(Ordering::Relaxed);
        loop {
            let current_val = f64::from_bits(current_bits);
            let next_val = current_val + delta;
            match self.bits.compare_exchange_weak(
                current_bits,
                next_val.to_bits(),
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_bits = actual,
            }
        }
    }
}

impl Default for AtomicFloat {
    fn default() -> Self {
        Self::new(0.0)
    }
}

/// Full real-time integrated Greeks exposure state (Spec §5.2).
pub struct RealTimeGreeksState {
    pub net_delta: AtomicFloat,
    pub net_gamma: AtomicFloat,
    pub net_vega: AtomicFloat,
    pub sofr_cash_balance: AtomicFloat,
}

impl RealTimeGreeksState {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            net_delta: AtomicFloat::new(0.0),
            net_gamma: AtomicFloat::new(0.0),
            net_vega: AtomicFloat::new(0.0),
            sofr_cash_balance: AtomicFloat::new(initial_cash),
        }
    }
}

impl Default for RealTimeGreeksState {
    fn default() -> Self {
        Self::new(100_000_000.0)
    }
}

/// 64-byte cache-line-aligned Greeks tracker to prevent false sharing (Spec §15).
#[repr(align(64))]
pub struct AlignedGreeksTracker {
    pub net_delta: AtomicU64,
    pub net_gamma: AtomicU64,
    pub net_vega: AtomicU64,
    pub sofr_cash: AtomicU64,
}

impl AlignedGreeksTracker {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            net_delta: AtomicU64::new(0.0f64.to_bits()),
            net_gamma: AtomicU64::new(0.0f64.to_bits()),
            net_vega: AtomicU64::new(0.0f64.to_bits()),
            sofr_cash: AtomicU64::new(initial_cash.to_bits()),
        }
    }

    #[inline(always)]
    pub fn load_delta(&self) -> f64 {
        f64::from_bits(self.net_delta.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub fn load_gamma(&self) -> f64 {
        f64::from_bits(self.net_gamma.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub fn load_vega(&self) -> f64 {
        f64::from_bits(self.net_vega.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub fn load_cash(&self) -> f64 {
        f64::from_bits(self.sofr_cash.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub fn add_delta(&self, val: f64) {
        Self::add_float(&self.net_delta, val);
    }

    #[inline(always)]
    pub fn add_gamma(&self, val: f64) {
        Self::add_float(&self.net_gamma, val);
    }

    #[inline(always)]
    pub fn add_vega(&self, val: f64) {
        Self::add_float(&self.net_vega, val);
    }

    #[inline(always)]
    pub fn add_cash(&self, val: f64) {
        Self::add_float(&self.sofr_cash, val);
    }

    #[inline(always)]
    pub fn update_greeks(&self, d_delta: f64, d_gamma: f64, d_vega: f64) {
        self.add_delta(d_delta);
        self.add_gamma(d_gamma);
        self.add_vega(d_vega);
    }

    #[inline(always)]
    fn add_float(target: &AtomicU64, val: f64) {
        let mut current_bits = target.load(Ordering::Relaxed);
        loop {
            let current_f64 = f64::from_bits(current_bits);
            let next_f64 = current_f64 + val;
            match target.compare_exchange_weak(
                current_bits,
                next_f64.to_bits(),
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_bits = actual,
            }
        }
    }
}

impl Default for AlignedGreeksTracker {
    fn default() -> Self {
        Self::new(100_000_000.0)
    }
}

/// Atomic portfolio state used in the integrated bookmaking engine (Spec §20).
///
/// Tracks both a global (all-asset) aggregate delta -- preserved for
/// backward compatibility with callers that have no per-asset context
/// (e.g. the FIX drop-copy parser, which does not currently decode
/// Tag 55/Symbol) -- and a per-asset delta map keyed by `PackedAssetKey`.
///
/// Per-asset tracking fixes Task 42: previously the orchestrator fed the
/// *global* aggregate delta into `Bookmaker::compute_quote` for every
/// instrument, so inventory accumulated in one asset (e.g. AAPL) would
/// incorrectly skew quotes for a completely unrelated asset (e.g. TSLA).
#[repr(align(64))]
pub struct AtomicPortfolioState {
    pub net_delta: AtomicU64,
    pub net_gamma: AtomicU64,
    pub net_vega: AtomicU64,
    pub sofr_cash: AtomicU64,
    /// Per-asset delta exposure, keyed by `PackedAssetKey.data`.
    per_asset_delta: RwLock<HashMap<u128, f64>>,
}

impl AtomicPortfolioState {
    pub fn new(initial_cash: f64) -> Self {
        Self {
            net_delta: AtomicU64::new(0.0f64.to_bits()),
            net_gamma: AtomicU64::new(0.0f64.to_bits()),
            net_vega: AtomicU64::new(0.0f64.to_bits()),
            sofr_cash: AtomicU64::new(initial_cash.to_bits()),
            per_asset_delta: RwLock::new(HashMap::new()),
        }
    }

    /// Default bucket for legacy, asset-agnostic delta updates.
    ///
    /// Matches the codebase-wide convention (CLI default symbol, ingestion
    /// fallback symbol, orchestrator demo symbol) that the single-asset
    /// legacy default instrument is AAPL on the NMS source.
    #[inline(always)]
    fn default_asset_key() -> PackedAssetKey {
        PackedAssetKey::new_equity(sources::NMS, "AAPL")
    }

    /// Load the global (all-asset) aggregate delta.
    #[inline(always)]
    pub fn load_delta(&self) -> f64 {
        f64::from_bits(self.net_delta.load(Ordering::Acquire))
    }

    /// Load the delta exposure attributed to a specific asset.
    ///
    /// Returns `0.0` if the asset has no recorded exposure.
    #[inline(always)]
    pub fn load_delta_for_asset(&self, asset_key: PackedAssetKey) -> f64 {
        let key_data: u128 = asset_key.data;
        self.per_asset_delta
            .read()
            .expect("per_asset_delta lock poisoned")
            .get(&key_data)
            .copied()
            .unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn load_cash(&self) -> f64 {
        f64::from_bits(self.sofr_cash.load(Ordering::Acquire))
    }

    /// Add to the global aggregate delta, attributing the change to the
    /// legacy default asset bucket (backward-compatible with callers that
    /// have no per-asset context, e.g. the drop-copy listener).
    #[inline(always)]
    pub fn add_delta(&self, val: f64) {
        self.add_delta_for_asset(Self::default_asset_key(), val);
    }

    /// Add to both the global aggregate delta and the per-asset delta for
    /// `asset_key`. This is the primary entry point for asset-aware callers
    /// (e.g. the orchestrator processing a tick for a known instrument).
    #[inline(always)]
    pub fn add_delta_for_asset(&self, asset_key: PackedAssetKey, val: f64) {
        Self::add_float_atomic(&self.net_delta, val);

        let key_data: u128 = asset_key.data;
        let mut map = self
            .per_asset_delta
            .write()
            .expect("per_asset_delta lock poisoned");
        *map.entry(key_data).or_insert(0.0) += val;
    }

    #[inline(always)]
    pub fn add_cash(&self, val: f64) {
        Self::add_float_atomic(&self.sofr_cash, val);
    }

    #[inline(always)]
    fn add_float_atomic(target: &AtomicU64, val: f64) {
        let mut bits = target.load(Ordering::Relaxed);
        loop {
            let current = f64::from_bits(bits);
            let next = current + val;
            match target.compare_exchange_weak(
                bits,
                next.to_bits(),
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => bits = actual,
            }
        }
    }
}

impl Default for AtomicPortfolioState {
    fn default() -> Self {
        Self::new(100_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_float_add() {
        let af = AtomicFloat::new(100.0);
        af.fetch_add(50.0);
        assert!((af.load() - 150.0).abs() < 1e-9);
    }

    #[test]
    fn aligned_tracker_delta() {
        let tracker = AlignedGreeksTracker::new(50_000_000.0);
        tracker.add_delta(100.0);
        tracker.add_delta(-30.0);
        assert!((tracker.load_delta() - 70.0).abs() < 1e-9);
        assert!((tracker.load_cash() - 50_000_000.0).abs() < 1e-9);
    }
}