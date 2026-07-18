//! Live Market Tick (Spec §16)
//!
//! Fixed-size, `Copy`-able tick struct that flows through the lock-free
//! ring buffer from the ingestion thread to the orchestration thread.

use crate::symbology::PackedAssetKey;

/// A live market tick extracted from the ingestion pipeline.
///
/// This struct is `Copy` and small enough to pass through a lock-free
/// `ArrayQueue` without allocation. It carries the minimum information
/// needed by the orchestrator to compute quotes and hedges.
#[derive(Debug, Clone, Copy)]
pub struct LiveMarketTick {
    /// 128-bit packed asset key identifying the instrument.
    pub asset_key: PackedAssetKey,
    /// Spot / mid price at the time of the tick.
    pub spot: f64,
    /// Strike price (0.0 for equities/futures).
    pub strike: f64,
    /// Time to expiry in years (0.0 for equities/futures).
    pub expiry: f64,
    /// Best bid price.
    pub bid_px: f64,
    /// Best bid size.
    pub bid_sz: f64,
    /// Best ask price.
    pub ask_px: f64,
    /// Best ask size.
    pub ask_sz: f64,
    /// Nanosecond timestamp from the exchange.
    pub timestamp_ns: u64,
}

impl LiveMarketTick {
    /// Create a new live market tick.
    #[inline(always)]
    pub fn new(
        asset_key: PackedAssetKey,
        spot: f64,
        bid_px: f64,
        bid_sz: f64,
        ask_px: f64,
        ask_sz: f64,
        timestamp_ns: u64,
    ) -> Self {
        Self {
            asset_key,
            spot,
            strike: 0.0,
            expiry: 0.0,
            bid_px,
            bid_sz,
            ask_px,
            ask_sz,
            timestamp_ns,
        }
    }

    /// Create a new live market tick for an option.
    #[inline(always)]
    pub fn new_option(
        asset_key: PackedAssetKey,
        spot: f64,
        strike: f64,
        expiry: f64,
        bid_px: f64,
        bid_sz: f64,
        ask_px: f64,
        ask_sz: f64,
        timestamp_ns: u64,
    ) -> Self {
        Self {
            asset_key,
            spot,
            strike,
            expiry,
            bid_px,
            bid_sz,
            ask_px,
            ask_sz,
            timestamp_ns,
        }
    }

    /// Compute the mid price from bid/ask.
    #[inline(always)]
    pub fn mid_price(&self) -> f64 {
        if self.bid_px > 0.0 && self.ask_px > 0.0 {
            (self.bid_px + self.ask_px) / 2.0
        } else if self.spot > 0.0 {
            self.spot
        } else {
            0.0
        }
    }

    /// Check if this is an option tick (has strike > 0).
    #[inline(always)]
    pub fn is_option(&self) -> bool {
        self.strike > 0.0
    }
}

impl Default for LiveMarketTick {
    fn default() -> Self {
        Self {
            asset_key: PackedAssetKey::default(),
            spot: 0.0,
            strike: 0.0,
            expiry: 0.0,
            bid_px: 0.0,
            bid_sz: 0.0,
            ask_px: 0.0,
            ask_sz: 0.0,
            timestamp_ns: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbology::sources;

    #[test]
    fn tick_mid_price() {
        let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
        let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);
        assert!((tick.mid_price() - 150.0).abs() < 1e-9);
    }

    #[test]
    fn tick_option_detection() {
        let key = PackedAssetKey::new_option(sources::NMS, "AAPL", 30, 15000, true);
        let tick = LiveMarketTick::new_option(key, 150.0, 150.0, 0.25, 5.0, 10, 5.1, 10, 1000);
        assert!(tick.is_option());
        assert!((tick.strike - 150.0).abs() < 1e-9);
    }

    #[test]
    fn tick_equity_not_option() {
        let key = PackedAssetKey::new_equity(sources::NMS, "AAPL");
        let tick = LiveMarketTick::new(key, 150.0, 149.98, 500.0, 150.02, 500.0, 1000);
        assert!(!tick.is_option());
    }
}