//! CMTA Post-Trade Clearing & Margin Sweep (Spec §6)
//!
//! Multi-strike option position aggregation, step-out netting,
//! cross-expiration compression, and EOD margin sweep to minimize
//! SPAN/TIMS haircut and deploy excess margin to bilateral repo /
//! SOFR overnight deposits.

pub mod cmta;
pub mod margin_sweep;

pub use cmta::{CmtaClearingEngine, OptionPosition, PositionNettingResult};
pub use margin_sweep::{MarginSweepEngine, SweepAction, SweepDestination};