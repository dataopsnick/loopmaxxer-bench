//! Gaussian Mixture Model (GMM) Hidden-State Module
//!
//! 3-component GMM modeling the order-flow generating process:
//! - Component 0: Noise trader (symmetric, no information)
//! - Component 1: Institutional buyer (persistent directional flow)
//! - Component 2: Informed insider (adverse selection, correlated with future returns)

pub mod em;
pub mod features;
pub mod model;

pub use em::GmmFitter;
pub use features::OrderFlowFeatures;
pub use model::{GmmComponent, GmmModel, TraderState};