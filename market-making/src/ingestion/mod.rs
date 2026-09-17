//! Hardware-Bypass Ingestion & NUMA Topology (Spec §4, §28, §31-34)
//!
//! Solarflare EF_VI userspace driver FFI bindings, page-aligned DMA frame
//! buffers, NUMA-aware thread pinning, and zero-copy SpiderStream header
//! overlay casting.

pub mod dma_buffer;
pub mod driver;
pub mod ef_vi;
pub mod numa;
pub mod spider_stream;

pub use dma_buffer::DmaFrameBuffer;
pub use driver::UserspaceIngestionDriver;
pub use numa::{pin_thread_to_core, NumaConfig};
pub use spider_stream::{SpiderStreamHeader, StockBookQuoteBody};
