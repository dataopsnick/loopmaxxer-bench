//! Smart Hedging Routing Matrix (Spec §22, §25)
//!
//! Quadratic-programming solver for optimal stock/future/basket hedge
//! allocation. Objective: min{ γ·hᵀΣh + hᵀc_spread + hᵀr_carry }
//! subject to wᵀh + ΔD = 0.

pub mod router;

pub use router::{HedgeAllocation, HedgingRoutingMatrix};