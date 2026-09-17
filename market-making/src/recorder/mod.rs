//! Columnar mmap Time-Series Recorder (Spec §10)
//!
//! `MappedColumnarWriter` using `mmap` + `O_DIRECT` for zero-syscall
//! trade logging. Columnar layout: `u64` timestamp, `f64` price, `u32` size.
//! `Drop` implementation with `munmap` cleanup. Overflow protection.

pub mod mapped_writer;

pub use mapped_writer::{MappedColumnarWriter, TradeRecord};