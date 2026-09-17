//! MLE Position Size Inference
//!
//! Infers the MLE position size of the delta-neutral level-IV market maker
//! by maximizing expected log-likelihood of P&L given the fitted GMM,
//! adverse selection costs, and SOFR carry.

pub mod likelihood;
pub mod position;

pub use likelihood::PositionLikelihood;
pub use position::{MlePositionInferer, MlePositionResult};