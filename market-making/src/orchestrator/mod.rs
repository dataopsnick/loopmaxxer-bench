//! Production Orchestrator (Spec §16, §29)
//!
//! Integrates all subsystems into a single NUMA-pinned spin loop:
//!   ingest → spline vol → reservation price + OFI → spread → risk gate → DMA submit
//!
//! Uses a 65k-capacity `ArrayQueue<LiveMarketTick>` lock-free ring buffer
//! for passing ticks from the ingestion thread to the orchestration thread.

pub mod live_tick;
pub mod orchestrator;

pub use live_tick::LiveMarketTick;
pub use orchestrator::{ActiveOrchestrator, OrchestratorConfig, OrchestratorStats};