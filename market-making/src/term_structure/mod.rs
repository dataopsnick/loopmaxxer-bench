//! Term-Structure Beta Estimator (Spec §14)
//!
//! Tracks the real-time covariance between near-month and next-month futures
//! prices to estimate the term-structure beta (β_term). When β_term collapses
//! below a critical threshold (contango → backwardation reversal), the
//! inventory risk penalty is widened to force near-month short liquidation.

pub mod estimator;

pub use estimator::{Regime, TermStructureBetaEstimator};