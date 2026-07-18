//! FIX 4.4 Binary Message Templates (Spec §8)
//!
//! Pre-allocated fixed-buffer FIX 4.4 message templates with offset-based
//! field stuffing. No `format!` or `write!` macros on the hot path —
//! variable fields (price, quantity, ClOrdID) are written via direct
//! pointer casting to pre-baked byte offsets.

use std::io::IoSlice;

/// SOH delimiter byte (0x01) used as FIX field separator.
pub const SOH: u8 = 0x01;

/// Maximum size of a FIX New Order Single message buffer.
pub const NOS_BUFFER_SIZE: usize = 256;

/// Offset constants for the New Order Single (MsgType=D) template.
///
/// Layout (Spec §8.1):
/// ```text
/// 0x00: "8=FIX.4.4" + SOH          (10 bytes)
/// 0x0A: "9=" + body_length + SOH    (variable, 8-byte slot)
/// 0x14: "35=D" + SOH               (5 bytes)
/// 0x19: "11=" + ClOrdID + SOH      (24-byte window)
/// 0x31: "21=2" + SOH               (5 bytes, HandlInst=2)
/// 0x36: "38=" + OrderQty + SOH     (12-byte slot)
/// 0x42: "40=2" + SOH               (5 bytes, OrdType=Limit)
/// 0x47: "44=" + Price + SOH         (16-byte slot)
/// 0x57: "54=" + Side + SOH         (4 bytes)
/// 0x5B: "10=" + CheckSum + SOH    (7 bytes)
/// ```
pub mod nos_offsets {
    /// Start of BeginString "8=FIX.4.4" + SOH
    pub const BEGIN_STRING: usize = 0;
    /// Start of BodyLength "9=" + 8-byte slot + SOH
    pub const BODY_LENGTH: usize = 10;
    /// Start of MsgType "35=D" + SOH
    pub const MSG_TYPE: usize = 19;
    /// Start of ClOrdID "11=" + 24-byte window + SOH
    pub const CL_ORD_ID: usize = 25;
    /// Start of HandlInst "21=2" + SOH
    pub const HANDL_INST: usize = 50;
    /// Start of OrderQty "38=" + 12-byte slot + SOH
    pub const ORDER_QTY: usize = 55;
    /// Start of OrdType "40=2" + SOH
    pub const ORD_TYPE: usize = 68;
    /// Start of Price "44=" + 16-byte slot + SOH
    pub const PRICE: usize = 73;
    /// Start of Side "54=" + 1 byte + SOH
    pub const SIDE: usize = 90;
    /// Start of CheckSum "10=" + 3 bytes + SOH
    pub const CHECK_SUM: usize = 94;
}

/// A pre-allocated fixed-buffer FIX 4.4 message template.
///
/// The static header fields are baked in at construction time.
/// Variable fields (ClOrdID, OrderQty, Price, Side) are stuffed
/// via direct byte writes to known offsets.
pub struct FixTemplateBuffer {
    buf: [u8; NOS_BUFFER_SIZE],
    len: usize,
}

impl FixTemplateBuffer {
    /// Create a new template buffer with the FIX 4.4 header pre-baked.
    pub fn new_nos_template() -> Self {
        let mut buf = [0u8; NOS_BUFFER_SIZE];

        // 8=FIX.4.4<SOH>
        buf[nos_offsets::BEGIN_STRING..nos_offsets::BODY_LENGTH]
            .copy_from_slice(b"8=FIX.4.4\x01");

        // 9=<SOH-padded 8-byte slot><SOH> (body length filled later)
        buf[nos_offsets::BODY_LENGTH] = b'9';
        buf[nos_offsets::BODY_LENGTH + 1] = b'=';
        // 8-byte slot for body length digits, then SOH at offset 18
        buf[nos_offsets::BODY_LENGTH + 2..nos_offsets::MSG_TYPE - 1]
            .copy_from_slice(b"        ");
        buf[nos_offsets::MSG_TYPE - 1] = SOH;

        // 35=D<SOH>
        buf[nos_offsets::MSG_TYPE..nos_offsets::CL_ORD_ID].copy_from_slice(b"35=D\x01");

        // 11=<24-byte ClOrdID window><SOH>
        buf[nos_offsets::CL_ORD_ID] = b'1';
        buf[nos_offsets::CL_ORD_ID + 1] = b'1';
        buf[nos_offsets::CL_ORD_ID + 2] = b'=';
        buf[nos_offsets::CL_ORD_ID + 3..nos_offsets::HANDL_INST - 1]
            .copy_from_slice(&[b'0'; 24]);
        buf[nos_offsets::HANDL_INST - 1] = SOH;

        // 21=2<SOH> (HandlInst = 2, AutomatedExecutionNoIntervention)
        buf[nos_offsets::HANDL_INST..nos_offsets::ORDER_QTY].copy_from_slice(b"21=2\x01");

        // 38=<12-byte OrderQty slot><SOH>
        buf[nos_offsets::ORDER_QTY] = b'3';
        buf[nos_offsets::ORDER_QTY + 1] = b'8';
        buf[nos_offsets::ORDER_QTY + 2] = b'=';
        buf[nos_offsets::ORDER_QTY + 3..nos_offsets::ORD_TYPE - 1]
            .copy_from_slice(&[b'0'; 12]);
        buf[nos_offsets::ORD_TYPE - 1] = SOH;

        // 40=2<SOH> (OrdType = 2, Limit)
        buf[nos_offsets::ORD_TYPE..nos_offsets::PRICE].copy_from_slice(b"40=2\x01");

        // 44=<16-byte Price slot><SOH>
        buf[nos_offsets::PRICE] = b'4';
        buf[nos_offsets::PRICE + 1] = b'4';
        buf[nos_offsets::PRICE + 2] = b'=';
        buf[nos_offsets::PRICE + 3..nos_offsets::SIDE - 1].copy_from_slice(&[b'0'; 16]);
        buf[nos_offsets::SIDE - 1] = SOH;

        // 54=<1-byte Side><SOH>
        buf[nos_offsets::SIDE] = b'5';
        buf[nos_offsets::SIDE + 1] = b'4';
        buf[nos_offsets::SIDE + 2] = b'=';
        buf[nos_offsets::SIDE + 3] = b'1'; // default Buy
        buf[nos_offsets::SIDE + 4] = SOH;

        // 10=<3-byte CheckSum><SOH>
        buf[nos_offsets::CHECK_SUM] = b'1';
        buf[nos_offsets::CHECK_SUM + 1] = b'0';
        buf[nos_offsets::CHECK_SUM + 2] = b'=';
        buf[nos_offsets::CHECK_SUM + 3..nos_offsets::CHECK_SUM + 6].copy_from_slice(b"000");
        buf[nos_offsets::CHECK_SUM + 6] = SOH;

        Self {
            buf,
            len: nos_offsets::CHECK_SUM + 7,
        }
    }

    /// Stuff the ClOrdID (Tag 11) into the pre-baked template slot.
    ///
    /// `cl_ord_id` is a numeric counter encoded as ASCII digits.
    #[inline(always)]
    pub fn set_cl_ord_id(&mut self, cl_ord_id: u64) {
        let slot = &mut self.buf[nos_offsets::CL_ORD_ID + 3..nos_offsets::HANDL_INST - 1];
        write_u64_ascii(slot, cl_ord_id);
    }

    /// Stuff the OrderQty (Tag 38) into the pre-baked template slot.
    #[inline(always)]
    pub fn set_order_qty(&mut self, qty: u32) {
        let slot = &mut self.buf[nos_offsets::ORDER_QTY + 3..nos_offsets::ORD_TYPE - 1];
        write_u32_ascii(slot, qty);
    }

    /// Stuff the Price (Tag 44) into the pre-baked template slot.
    ///
    /// Price is encoded as fixed-point with 4 decimal places.
    #[inline(always)]
    pub fn set_price(&mut self, price_usd: f64) {
        let slot = &mut self.buf[nos_offsets::PRICE + 3..nos_offsets::SIDE - 1];
        // Encode as price * 10000 (fixed-point, 4 decimals)
        let price_cents = (price_usd * 10000.0) as u64;
        write_u64_ascii(slot, price_cents);
    }

    /// Stuff the Side (Tag 54) into the pre-baked template slot.
    ///
    /// `side`: `true` = Buy ('1'), `false` = Sell ('2').
    #[inline(always)]
    pub fn set_side(&mut self, is_buy: bool) {
        self.buf[nos_offsets::SIDE + 3] = if is_buy { b'1' } else { b'2' };
    }

    /// Compute and stuff the body length (Tag 9).
    ///
    /// Body length = bytes from MsgType (Tag 35) to just before CheckSum (Tag 10).
    #[inline(always)]
    pub fn finalize_body_length(&mut self) {
        let body_len = (nos_offsets::CHECK_SUM - nos_offsets::MSG_TYPE) as u64;
        let slot = &mut self.buf[nos_offsets::BODY_LENGTH + 2..nos_offsets::MSG_TYPE - 1];
        write_u64_ascii(slot, body_len);
    }

    /// Compute and stuff the FIX checksum (Tag 10).
    ///
    /// Checksum = (sum of all bytes up to and including SOH before Tag 10) mod 256,
    /// encoded as 3 ASCII digits.
    #[inline(always)]
    pub fn finalize_checksum(&mut self) {
        let mut sum: u32 = 0;
        for i in 0..nos_offsets::CHECK_SUM {
            sum = sum.wrapping_add(self.buf[i] as u32);
        }
        let chk = sum % 256;
        let slot = &mut self.buf[nos_offsets::CHECK_SUM + 3..nos_offsets::CHECK_SUM + 6];
        slot[0] = b'0' + ((chk / 100) as u8);
        slot[1] = b'0' + (((chk / 10) % 10) as u8);
        slot[2] = b'0' + ((chk % 10) as u8);
    }

    /// Get the finalized message as a byte slice.
    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    /// Get an `IoSlice` for zero-copy `sendto` integration.
    #[inline(always)]
    pub fn as_io_slice(&self) -> IoSlice<'_> {
        IoSlice::new(self.as_bytes())
    }

    /// Get the raw buffer (for testing).
    pub fn raw_buffer(&self) -> &[u8; NOS_BUFFER_SIZE] {
        &self.buf
    }
}

/// A FIX 4.4 New Order Single message builder.
pub struct FixNewOrderSingle {
    template: FixTemplateBuffer,
}

impl FixNewOrderSingle {
    /// Create a new NOS builder with the template pre-baked.
    pub fn new() -> Self {
        Self {
            template: FixTemplateBuffer::new_nos_template(),
        }
    }

    /// Set the ClOrdID.
    #[inline(always)]
    pub fn cl_ord_id(mut self, id: u64) -> Self {
        self.template.set_cl_ord_id(id);
        self
    }

    /// Set the order quantity.
    #[inline(always)]
    pub fn order_qty(mut self, qty: u32) -> Self {
        self.template.set_order_qty(qty);
        self
    }

    /// Set the limit price.
    #[inline(always)]
    pub fn price(mut self, price_usd: f64) -> Self {
        self.template.set_price(price_usd);
        self
    }

    /// Set the side (true = Buy, false = Sell).
    #[inline(always)]
    pub fn side(mut self, is_buy: bool) -> Self {
        self.template.set_side(is_buy);
        self
    }

    /// Finalize the message (compute body length + checksum) and return bytes.
    pub fn build(mut self) -> Vec<u8> {
        self.template.finalize_body_length();
        self.template.finalize_checksum();
        self.template.as_bytes().to_vec()
    }

    /// Finalize and return an `IoSlice` (zero-copy, borrows from self).
    pub fn build_io_slice(&mut self) -> IoSlice<'_> {
        self.template.finalize_body_length();
        self.template.finalize_checksum();
        self.template.as_io_slice()
    }
}

impl Default for FixNewOrderSingle {
    fn default() -> Self {
        Self::new()
    }
}

/// Write a `u64` as right-aligned ASCII digits into a fixed-width slot,
/// left-padded with '0'. No heap allocation.
#[inline(always)]
fn write_u64_ascii(slot: &mut [u8], val: u64) {
    let len = slot.len();
    let mut v = val;
    // Write digits right-to-left
    for i in (0..len).rev() {
        slot[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            // Fill remaining with '0' padding
            for j in 0..i {
                slot[j] = b'0';
            }
            break;
        }
    }
}

/// Write a `u32` as right-aligned ASCII digits into a fixed-width slot.
#[inline(always)]
fn write_u32_ascii(slot: &mut [u8], val: u32) {
    let len = slot.len();
    let mut v = val;
    for i in (0..len).rev() {
        slot[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            for j in 0..i {
                slot[j] = b'0';
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nos_template_has_fix_header() {
        let buf = FixTemplateBuffer::new_nos_template();
        let bytes = buf.as_bytes();
        // Should start with "8=FIX.4.4<SOH>"
        assert_eq!(&bytes[0..10], b"8=FIX.4.4\x01");
        // Should contain "35=D<SOH>"
        assert!(bytes.windows(5).any(|w| w == b"35=D\x01"));
    }

    #[test]
    fn nos_set_side_buy_sell() {
        let mut buf = FixTemplateBuffer::new_nos_template();
        buf.set_side(true);
        assert_eq!(buf.raw_buffer()[nos_offsets::SIDE + 3], b'1');
        buf.set_side(false);
        assert_eq!(buf.raw_buffer()[nos_offsets::SIDE + 3], b'2');
    }

    #[test]
    fn nos_set_order_qty() {
        let mut buf = FixTemplateBuffer::new_nos_template();
        buf.set_order_qty(100);
        let slot = &buf.raw_buffer()[nos_offsets::ORDER_QTY + 3..nos_offsets::ORD_TYPE - 1];
        // Should contain "100" right-aligned
        let s = std::str::from_utf8(slot).unwrap();
        assert!(s.trim_end_matches('0').ends_with("100") || s.ends_with("100"));
    }

    #[test]
    fn nos_checksum_valid() {
        let mut buf = FixTemplateBuffer::new_nos_template();
        buf.set_cl_ord_id(42);
        buf.set_order_qty(100);
        buf.set_price(150.25);
        buf.set_side(true);
        buf.finalize_body_length();
        buf.finalize_checksum();

        // Verify checksum: sum of all bytes before Tag 10 mod 256
        let mut sum: u32 = 0;
        for i in 0..nos_offsets::CHECK_SUM {
            sum = sum.wrapping_add(buf.raw_buffer()[i] as u32);
        }
        let expected = sum % 256;
        let chk_bytes = &buf.raw_buffer()[nos_offsets::CHECK_SUM + 3..nos_offsets::CHECK_SUM + 6];
        let chk_val = ((chk_bytes[0] - b'0') as u32) * 100
            + ((chk_bytes[1] - b'0') as u32) * 10
            + (chk_bytes[2] - b'0') as u32;
        assert_eq!(chk_val, expected);
    }

    #[test]
    fn nos_builder_produces_message() {
        let msg = FixNewOrderSingle::new()
            .cl_ord_id(1)
            .order_qty(100)
            .price(150.25)
            .side(true)
            .build();
        assert!(msg.starts_with(b"8=FIX.4.4\x01"));
        assert!(msg.contains(&b'D')); // MsgType=D
    }

    #[test]
    fn write_u64_ascii_pads_correctly() {
        let mut slot = [b'X'; 8];
        write_u64_ascii(&mut slot, 42);
        assert_eq!(&slot, b"00000042");
    }
}