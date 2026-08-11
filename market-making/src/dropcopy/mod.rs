//! FIX Drop Copy Closed-Loop Listener (Spec §19, §20)
//!
//! Zero-allocation byte-scanning FIX 4.4 parser for ExecutionReport (MsgType=8)
//! messages. Extracts Tag 35, 150, 32, 54 via SOH-delimited scanning without
//! `String` allocation. Direct atomic CAS update of `AtomicPortfolioState.net_delta`.

pub mod listener;

pub use listener::RawDropCopyListener;