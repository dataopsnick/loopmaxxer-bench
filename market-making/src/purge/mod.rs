//! SQF Purge Driver & COB Legging Defense (Spec §9)
//!
//! Ultra-low-latency mass-cancel (SQF purge) driver and Complex Order Book
//! (COB) legging-out arbitrage defense via asymmetric skew repositioning.

pub mod cob_defense;
pub mod sqf_driver;

pub use cob_defense::{CobDefenseController, CobSpreadQuote, SkewAdjustment};
pub use sqf_driver::{LowLatencyPurgeDriver, SQFPurgeRequest};