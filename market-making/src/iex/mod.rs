//! IEX Historical Data Module
//!
//! Downloader and parser for IEX historical PCAP data from
//! https://iextrading.com/trading/market-data/#hist-download
//! Also includes a CSV fallback reader for testing.

pub mod downloader;
pub mod parser;

use serde::{Deserialize, Serialize};

/// A market event extracted from IEX historical data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    /// Top-of-book quote update (IEX TOPS)
    QuoteUpdate {
        symbol: String,
        bid_price: f64,
        bid_size: f64,
        ask_price: f64,
        ask_size: f64,
        timestamp_ns: u64,
    },
    /// Depth-of-book price level update (IEX DEEP)
    PriceLevelUpdate {
        symbol: String,
        side: BookSide,
        price: f64,
        size: f64,
        timestamp_ns: u64,
    },
    /// Trade / last sale event
    Trade {
        symbol: String,
        price: f64,
        size: f64,
        timestamp_ns: u64,
    },
}

/// Book side for depth updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BookSide {
    Bid,
    Ask,
}

/// Normalized top-of-book snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopOfBook {
    pub symbol: String,
    pub bid_price: f64,
    pub bid_size: f64,
    pub ask_price: f64,
    pub ask_size: f64,
    pub timestamp_ns: u64,
}

impl TopOfBook {
    #[inline(always)]
    pub fn mid_price(&self) -> f64 {
        (self.bid_price + self.ask_price) / 2.0
    }

    #[inline(always)]
    pub fn spread(&self) -> f64 {
        self.ask_price - self.bid_price
    }
}