//! Library entry point for the mr_market framework (Spec §16, §29) [PLAN].
//!
//! Exposes all submodules to integration tests, benchmarks, and the main binary.

pub mod bookmaker;
pub mod clearing;
pub mod codec;
pub mod dropcopy;
pub mod gmm;
pub mod hedging;
pub mod iex;
pub mod ingestion;
pub mod margin;
pub mod memorydb;
pub mod mle;
pub mod ofi;
pub mod orchestrator;
pub mod portfolio;
pub mod pricer;
pub mod purge;
pub mod recorder;
pub mod risk_gate;
pub mod simulation;
pub mod sofr;
pub mod symbology;
pub mod term_structure;
pub mod vol_surface;
