//! SBE (Simple Binary Encoding) Template Encoder (Spec §8)
//!
//! SBE template blitting for outbound New Order Single (NOS) and
//! mass-cancel messages. Uses direct memory blitting with `#[repr(C)]`
//! structs for zero-copy serialization.

use std::io::IoSlice;

/// SBE message header (Spec §8, SpiderStream-compatible).
///
/// All fields are little-endian, `#[repr(C, packed)]` for direct
/// memory overlay casting from DMA buffers.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct SbeMessageHeader {
    /// Schema ID (identifies the message template).
    pub schema_id: u16,
    /// Template ID (e.g., 1 = NewOrderSingle, 2 = MassCancel).
    pub template_id: u16,
    /// Message sequence number.
    pub sequence_number: u32,
    /// Nanosecond timestamp.
    pub sent_time_ns: u64,
    /// Message body length (bytes after header).
    pub body_length: u16,
}

impl SbeMessageHeader {
    /// Size of the header in bytes.
    pub const SIZE: usize = std::mem::size_of::<Self>();

    /// Create a new SBE header.
    #[inline(always)]
    pub fn new(schema_id: u16, template_id: u16, sequence_number: u32, sent_time_ns: u64, body_length: u16) -> Self {
        Self {
            schema_id,
            template_id,
            sequence_number,
            sent_time_ns,
            body_length,
        }
    }
}

/// SBE New Order Single message body (Spec §8).
///
/// Fixed-size fields for zero-copy blitting. Variable-length fields
/// (symbol, account) are fixed-width space-padded.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct SbeNewOrderSingle {
    /// SBE message header.
    pub header: SbeMessageHeader,
    /// Order ID (ClOrdID equivalent).
    pub client_order_id: u64,
    /// Space-padded symbol (12 bytes).
    pub symbol: [u8; 12],
    /// Side: 1 = Buy, 2 = Sell.
    pub side: u8,
    /// Order quantity.
    pub order_qty: u32,
    /// Limit price (fixed-point, 4 decimals).
    pub price: u64,
    /// Order type: 2 = Limit.
    pub order_type: u8,
    /// Time in force: 0 = Day, 1 = GTC, 3 = IOC, 4 = FOK.
    pub time_in_force: u8,
}

impl SbeNewOrderSingle {
    /// Total size of the message including header.
    pub const SIZE: usize = std::mem::size_of::<Self>();

    /// Create a new SBE New Order Single.
    #[inline(always)]
    pub fn new(
        sequence_number: u32,
        sent_time_ns: u64,
        client_order_id: u64,
        symbol: &str,
        side: u8,
        order_qty: u32,
        price: f64,
    ) -> Self {
        let mut sym = [b' '; 12];
        let bytes = symbol.as_bytes();
        let len = bytes.len().min(12);
        sym[..len].copy_from_slice(&bytes[..len]);

        Self {
            header: SbeMessageHeader::new(1, 1, sequence_number, sent_time_ns, (Self::SIZE - SbeMessageHeader::SIZE) as u16),
            client_order_id,
            symbol: sym,
            side,
            order_qty,
            price: (price * 10000.0) as u64,
            order_type: 2, // Limit
            time_in_force: 0, // Day
        }
    }

    /// Serialize to a byte slice via zero-copy pointer cast.
    ///
    /// # Safety
    /// The struct is `#[repr(C, packed)]`, so its memory layout is
    /// well-defined and can be safely reinterpreted as bytes.
    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: Self is #[repr(C, packed)] with no padding, so the
        // memory layout is contiguous and well-defined.
        unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }

    /// Get an `IoSlice` for zero-copy `sendto` integration.
    #[inline(always)]
    pub fn as_io_slice(&self) -> IoSlice<'_> {
        IoSlice::new(self.as_bytes())
    }
}

/// SBE Mass Cancel request message body (Spec §9).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct SbeMassCancelRequest {
    /// SBE message header.
    pub header: SbeMessageHeader,
    /// Space-padded client firm (8 bytes).
    pub client_firm: [u8; 8],
    /// Space-padded account (8 bytes).
    pub account: [u8; 8],
    /// Space-padded underlying symbol (12 bytes).
    pub underlying: [u8; 12],
    /// Purge group ID.
    pub purge_group_id: u32,
    /// Nanosecond sending time.
    pub sending_time_ns: u64,
}

impl SbeMassCancelRequest {
    /// Total size of the message including header.
    pub const SIZE: usize = std::mem::size_of::<Self>();

    /// Create a new SBE mass cancel request.
    #[inline(always)]
    pub fn new(
        sequence_number: u32,
        sent_time_ns: u64,
        client_firm: &str,
        account: &str,
        underlying: &str,
        purge_group_id: u32,
    ) -> Self {
        let mut firm = [b' '; 8];
        let mut acc = [b' '; 8];
        let mut und = [b' '; 12];

        let fb = client_firm.as_bytes();
        firm[..fb.len().min(8)].copy_from_slice(&fb[..fb.len().min(8)]);

        let ab = account.as_bytes();
        acc[..ab.len().min(8)].copy_from_slice(&ab[..ab.len().min(8)]);

        let ub = underlying.as_bytes();
        und[..ub.len().min(12)].copy_from_slice(&ub[..ub.len().min(12)]);

        Self {
            header: SbeMessageHeader::new(1, 2, sequence_number, sent_time_ns, (Self::SIZE - SbeMessageHeader::SIZE) as u16),
            client_firm: firm,
            account: acc,
            underlying: und,
            purge_group_id,
            sending_time_ns: sent_time_ns,
        }
    }

    /// Serialize to a byte slice via zero-copy pointer cast.
    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: Self is #[repr(C, packed)] with no padding.
        unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

/// SBE encoder for outbound messages.
pub struct SbeEncoder {
    sequence_counter: std::sync::atomic::AtomicU64,
}

impl SbeEncoder {
    /// Create a new SBE encoder with sequence starting at 1.
    pub fn new() -> Self {
        Self {
            sequence_counter: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Get the next sequence number.
    #[inline(always)]
    pub fn next_sequence(&self) -> u32 {
        self.sequence_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u32
    }

    /// Encode a New Order Single message.
    #[inline(always)]
    pub fn encode_new_order_single(
        &self,
        client_order_id: u64,
        symbol: &str,
        side: u8,
        order_qty: u32,
        price: f64,
        sent_time_ns: u64,
    ) -> SbeNewOrderSingle {
        let seq = self.next_sequence();
        SbeNewOrderSingle::new(seq, sent_time_ns, client_order_id, symbol, side, order_qty, price)
    }

    /// Encode a mass cancel request.
    #[inline(always)]
    pub fn encode_mass_cancel(
        &self,
        client_firm: &str,
        account: &str,
        underlying: &str,
        purge_group_id: u32,
        sent_time_ns: u64,
    ) -> SbeMassCancelRequest {
        let seq = self.next_sequence();
        SbeMassCancelRequest::new(seq, sent_time_ns, client_firm, account, underlying, purge_group_id)
    }
}

impl Default for SbeEncoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sbe_nos_size_is_consistent() {
        // The struct should have a well-defined size
        let size = std::mem::size_of::<SbeNewOrderSingle>();
        assert!(size > 0);
        assert!(size >= SbeMessageHeader::SIZE);
    }

    #[test]
    fn sbe_nos_roundtrip_bytes() {
        let nos = SbeNewOrderSingle::new(1, 1000, 42, "AAPL", 1, 100, 150.25);
        let bytes = nos.as_bytes();
        assert_eq!(bytes.len(), SbeNewOrderSingle::SIZE);

        // Verify symbol is space-padded
        let sym_start = SbeMessageHeader::SIZE + 8; // after header + client_order_id
        let sym_end = sym_start + 12;
        let sym = &bytes[sym_start..sym_end];
        assert_eq!(&sym[..4], b"AAPL");
    }

    #[test]
    fn sbe_mass_cancel_roundtrip() {
        let mc = SbeMassCancelRequest::new(1, 2000, "SRCORE", "T.ACC", "AAPL", 1);
        let bytes = mc.as_bytes();
        assert_eq!(bytes.len(), SbeMassCancelRequest::SIZE);
    }

    #[test]
    fn sbe_encoder_increments_sequence() {
        let encoder = SbeEncoder::new();
        let s1 = encoder.next_sequence();
        let s2 = encoder.next_sequence();
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
    }

    #[test]
    fn sbe_encoder_produces_nos() {
        let encoder = SbeEncoder::new();
        let nos = encoder.encode_new_order_single(1, "AAPL", 1, 100, 150.0, 1000);
        assert_eq!(nos.header.template_id, 1);
        assert_eq!(nos.client_order_id, 1);
        assert_eq!(&nos.symbol[..4], b"AAPL");
        assert_eq!(nos.side, 1);
        assert_eq!(nos.order_qty, 100);
        assert_eq!(nos.price, 1500000); // 150.0 * 10000
    }
}