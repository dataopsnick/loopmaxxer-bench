//! Zero-Copy SBE/FIX Binary Serialization (Spec §8)
//!
//! Pre-allocated fixed-buffer FIX 4.4 message templates with offset-based
//! field stuffing (no `format!`/`write!` on hot path). SBE template blitting
//! for outbound New Order Single (NOS) and mass-cancel messages.

pub mod fix_template;
pub mod sbe_encoder;

pub use fix_template::{FixNewOrderSingle, FixTemplateBuffer, SOH};
pub use sbe_encoder::{SbeEncoder, SbeMessageHeader, SbeNewOrderSingle};