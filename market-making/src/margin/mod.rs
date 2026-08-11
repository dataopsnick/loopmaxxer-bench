//! Real-Time TIMS/SPAN Margin Modeling (Spec §12)
//!
//! 17-scenario TIMS stress grid, cross-asset delta/gamma netting,
//! negative margin cost weighting, and SPAN opportunity-cost tracking.

pub mod tims;

pub use tims::{MarginScenario, PositionGreeks, TimsMarginModel, TimsResult};