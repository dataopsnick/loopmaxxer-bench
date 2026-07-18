//! SpiderStream Binary Header Overlay Casting (Spec §33)
//!
//! Zero-copy `#[repr(C, packed)]` struct definitions for direct memory
//! overlay casting from DMA packet buffers. Enables sub-40ns parsing
//! of inbound SpiderStream SBE messages.

/// SpiderStream binary SBE message header (Spec §33).
///
/// Direct struct overlay cast from the DMA packet payload
/// (after Ethernet + IP + UDP headers are stripped).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct SpiderStreamHeader {
    /// System environment code.
    pub sys_environment: u8,
    /// Message type (SBE schema ID, e.g., 1050 = StkNbboQuoteA).
    pub message_type: u16,
    /// Source ID.
    pub source_id: u32,
    /// Sequence number.
    pub sequence_number: u32,
    /// Nanosecond sent timestamp.
    pub sent_time: u64,
    /// Message body length.
    pub message_length: u16,
    /// Key length.
    pub key_length: u16,
}

impl SpiderStreamHeader {
    /// Size of the header in bytes.
    pub const SIZE: usize = std::mem::size_of::<Self>();

    /// Ethernet + IP + UDP header offset (14 + 20 + 8 = 42 bytes).
    pub const NET_HEADER_OFFSET: usize = 42;
}

/// Stock book quote body (Spec §33, message_type = 1050).
///
/// Follows the SpiderStreamHeader + 12-byte symbol key in the packet.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct StockBookQuoteBody {
    /// Bid price.
    pub bid_price: f64,
    /// Ask price.
    pub ask_price: f64,
    /// Bid size.
    pub bid_size: i32,
    /// Ask size.
    pub ask_size: i32,
}

impl StockBookQuoteBody {
    /// Size of the body in bytes.
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

/// Option book quote body (Spec §23, msgoptionbookquote).
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct OptionBookQuoteBody {
    /// Bid price.
    pub bid_price: f64,
    /// Ask price.
    pub ask_price: f64,
    /// Bid size.
    pub bid_size: i32,
    /// Ask size.
    pub ask_size: i32,
    /// Implied bid volatility.
    pub bid_vol: f64,
    /// Implied ask volatility.
    pub ask_vol: f64,
}

/// Zero-copy cast a byte slice to a SpiderStreamHeader reference.
///
/// # Safety
/// The caller must ensure the slice is at least `SpiderStreamHeader::SIZE`
/// bytes and is properly aligned (or use `read_unaligned`).
#[inline(always)]
pub unsafe fn cast_header(ptr: *const u8) -> &'static SpiderStreamHeader {
    &*(ptr as *const SpiderStreamHeader)
}

/// Zero-copy cast a byte slice to a StockBookQuoteBody reference.
#[inline(always)]
pub unsafe fn cast_stock_quote(ptr: *const u8) -> &'static StockBookQuoteBody {
    &*(ptr as *const StockBookQuoteBody)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_reasonable() {
        let size = SpiderStreamHeader::SIZE;
        assert!(size > 0 && size <= 32, "Header size {} should be <= 32", size);
    }

    #[test]
    fn stock_quote_body_size() {
        let size = StockBookQuoteBody::SIZE;
        assert!(size >= 24, "Body size {} should be >= 24", size);
    }

    #[test]
    fn net_header_offset_correct() {
        assert_eq!(SpiderStreamHeader::NET_HEADER_OFFSET, 42);
    }
}