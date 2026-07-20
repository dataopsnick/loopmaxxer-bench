//! Raw Drop Copy Listener (Spec §19, §20)
//!
//! Zero-allocation byte-scanning FIX 4.4 parser that listens on a TCP
//! drop-copy port, extracts fill information (Tag 32, 54) from
//! ExecutionReport (MsgType=8) messages, and updates the atomic
//! portfolio state via lock-free CAS within <5µs of fill.

use std::io::Read;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::portfolio::AtomicPortfolioState;

/// SOH delimiter byte (0x01).
const SOH: u8 = 0x01;

/// Zero-allocation FIX 4.4 drop-copy listener (Spec §19, §20).
///
/// Connects to a TCP drop-copy port and scans incoming byte streams
/// for ExecutionReport messages. Extracts fill quantity (Tag 32) and
/// side (Tag 54) without any `String` allocation, then updates the
/// `AtomicPortfolioState.net_delta` via lock-free CAS.
pub struct RawDropCopyListener {
    stream: TcpStream,
    portfolio_state: Arc<AtomicPortfolioState>,
    /// Total number of fill events processed.
    fill_count: AtomicU64,
}

impl RawDropCopyListener {
    /// Create a new drop-copy listener connected to the given address.
    ///
    /// The listener will update `portfolio_state` on each fill.
    pub fn new(address: &str, state: Arc<AtomicPortfolioState>) -> std::io::Result<Self> {
        let stream = TcpStream::connect(address)?;
        stream.set_nonblocking(false)?;
        Ok(Self {
            stream,
            portfolio_state: state,
            fill_count: AtomicU64::new(0),
        })
    }

    /// Start the blocking listening loop.
    ///
    /// Reads from the TCP stream into a stack-allocated buffer, scans
    /// for FIX message boundaries, and processes ExecutionReport frames.
    /// Handles fragmented frame carry-over for partial TCP reads.
    pub fn start_listening_loop(&mut self) {
        let mut buffer = [0u8; 8192];
        let mut bytes_left = 0;

        loop {
            match self.stream.read(&mut buffer[bytes_left..]) {
                Ok(0) => {
                    eprintln!("[DROP COPY] Disconnected from transaction desk");
                    break;
                }
                Ok(read_bytes) => {
                    let total_bytes = bytes_left + read_bytes;
                    let mut cursor = 0;

                    // Scan for complete FIX messages
                    while cursor < total_bytes {
                        if let Some(msg_len) =
                            Self::locate_fix_message_bounds(&buffer[cursor..total_bytes])
                        {
                            let msg_slice = &buffer[cursor..(cursor + msg_len)];
                            self.process_raw_execution_frame(msg_slice);
                            cursor += msg_len;
                        } else {
                            break;
                        }
                    }

                    // Carry over remaining fragmentary bytes
                    if cursor < total_bytes {
                        buffer.copy_within(cursor..total_bytes, 0);
                        bytes_left = total_bytes - cursor;
                    } else {
                        bytes_left = 0;
                    }
                }
                Err(e) => {
                    eprintln!("[DROP COPY] Real-time read error: {:?}", e);
                    break;
                }
            }
        }
    }

    /// Locate the bounds of a complete FIX message in the byte buffer.
    ///
    /// Returns `Some(len)` if a complete message starting at "8=FIX."
    /// is found, where `len` is the byte offset to the start of the
    /// next message. Returns `None` if no complete message is found.
    #[inline(always)]
    fn locate_fix_message_bounds(data: &[u8]) -> Option<usize> {
        if data.len() < 10 {
            return None;
        }

        // Check for "8=FIX." prefix
        if &data[0..6] != b"8=FIX." {
            return None;
        }

        // Search for the next "8=FIX." which marks the start of the next message
        for i in 9..data.len() {
            if i + 6 < data.len() && data[i] == SOH && &data[i + 1..i + 7] == b"8=FIX." {
                return Some(i + 1);
            }
        }

        // If no next message found, check if we have a complete message
        // by looking for the CheckSum tag "10="
        for i in 9..data.len().saturating_sub(7) {
            if &data[i..i + 3] == b"10=" {
                // Find the SOH after the checksum
                for j in i + 3..data.len() {
                    if data[j] == SOH {
                        return Some(j + 1);
                    }
                }
            }
        }

        None
    }

    /// Process a raw FIX ExecutionReport frame.
    ///
    /// Extracts Tag 35 (MsgType), Tag 150 (ExecType), Tag 32 (LastQty),
    /// and Tag 54 (Side) via zero-allocation byte scanning.
    #[inline(always)]
    fn process_raw_execution_frame(&self, frame: &[u8]) {
        // Tag 35 (MsgType) — must be '8' for ExecutionReport
        if let Some(msg_type_idx) = Self::find_tag_offset(frame, b"35=") {
            if msg_type_idx >= frame.len() {
                return;
            }
            let msg_type = frame[msg_type_idx];
            if msg_type != b'8' {
                return; // Not an ExecutionReport
            }

            // Tag 150 (ExecType) — '2' = Filled, '1' = Partial Fill, 'F' = Trade
            if let Some(exec_type_idx) = Self::find_tag_offset(frame, b"150=") {
                if exec_type_idx >= frame.len() {
                    return;
                }
                let exec_type = frame[exec_type_idx];
                if exec_type == b'2' || exec_type == b'1' || exec_type == b'F' {
                    // Extract Tag 54 (Side): '1' = Buy, '2' = Sell
                    let side = Self::get_tag_char(frame, b"54=").unwrap_or(b'0');

                    // Extract Tag 32 (LastQty) as f64
                    let last_qty = Self::get_tag_float_value(frame, b"32=").unwrap_or(0.0);

                    if last_qty > 0.0 {
                        // Side multiplier: Buy = +1 (long delta), Sell = -1 (short delta)
                        let side_multiplier = if side == b'1' { 1.0 } else { -1.0 };

                        // Lock-free CAS update of net_delta
                        self.portfolio_state.add_delta(last_qty * side_multiplier);
                        self.fill_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    }

    /// Find the offset of a FIX tag pattern in the frame.
    ///
    /// Returns the index of the byte immediately after the pattern
    /// (i.e., the start of the tag's value).
    #[inline(always)]
    fn find_tag_offset(frame: &[u8], tag_pattern: &[u8]) -> Option<usize> {
        let pattern_len = tag_pattern.len();
        if frame.len() < pattern_len {
            return None;
        }
        for i in 0..=(frame.len() - pattern_len) {
            if &frame[i..(i + pattern_len)] == tag_pattern {
                return Some(i + pattern_len);
            }
        }
        None
    }

    /// Get a single character value for a FIX tag.
    #[inline(always)]
    fn get_tag_char(frame: &[u8], tag_pattern: &[u8]) -> Option<u8> {
        Self::find_tag_offset(frame, tag_pattern).map(|idx| {
            if idx < frame.len() {
                frame[idx]
            } else {
                0
            }
        })
    }

    /// Parse a FIX tag value as an f64 without `String` allocation.
    ///
    /// Manually scans digits and decimal point, avoiding `str::parse`.
    #[inline(always)]
    fn get_tag_float_value(frame: &[u8], tag_pattern: &[u8]) -> Option<f64> {
        let offset = Self::find_tag_offset(frame, tag_pattern)?;
        let mut val = 0.0f64;
        let mut decimal_found = false;
        let mut divisor = 1.0f64;

        for i in offset..frame.len() {
            let byte = frame[i];
            if byte == SOH {
                break; // End of tag value
            }
            if byte == b'.' {
                decimal_found = true;
                continue;
            }
            if byte >= b'0' && byte <= b'9' {
                let digit = (byte - b'0') as f64;
                if !decimal_found {
                    val = val * 10.0 + digit;
                } else {
                    divisor *= 10.0;
                    val = val + digit / divisor;
                }
            }
        }
        Some(val)
    }

    /// Get the total number of fills processed.
    #[inline(always)]
    pub fn fill_count(&self) -> u64 {
        self.fill_count.load(Ordering::Relaxed)
    }

    /// Process a single frame without a TCP connection (for testing).
    ///
    /// This is used by unit tests to verify the parser logic without
    /// requiring a live TCP drop-copy feed.
    pub fn process_frame_for_test(&self, frame: &[u8]) {
        self.process_raw_execution_frame(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_exec_report(side: u8, qty: f64) -> Vec<u8> {
        let side_str = if side == b'1' { "1" } else { "2" };
        let msg = format!(
            "8=FIX.4.4\x0135=8\x01150=2\x0154={}\x0132={}\x0110=000\x01",
            side_str, qty
        );
        msg.into_bytes()
    }

    #[test]
    fn find_tag_offset_basic() {
        let frame = b"8=FIX.4.4\x0135=8\x01150=2\x01";
        let idx = RawDropCopyListener::find_tag_offset(frame, b"35=");
        assert!(idx.is_some());
        assert_eq!(frame[idx.unwrap()], b'8');
    }

    #[test]
    fn find_tag_offset_not_found() {
        let frame = b"8=FIX.4.4\x0135=8\x01";
        let idx = RawDropCopyListener::find_tag_offset(frame, b"99=");
        assert!(idx.is_none());
    }

    #[test]
    fn get_tag_float_value_integer() {
        let frame = b"8=FIX.4.4\x0132=100\x01";
        let val = RawDropCopyListener::get_tag_float_value(frame, b"32=");
        assert!((val.unwrap() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn get_tag_float_value_decimal() {
        let frame = b"8=FIX.4.4\x0132=150.25\x01";
        let val = RawDropCopyListener::get_tag_float_value(frame, b"32=");
        assert!((val.unwrap() - 150.25).abs() < 1e-9);
    }

    #[test]
    fn process_buy_fill_updates_delta() {
        let state = Arc::new(AtomicPortfolioState::new(100_000_000.0));
        let listener = create_test_listener(state.clone());

        let frame = make_exec_report(b'1', 100.0);
        listener.process_frame_for_test(&frame);

        let delta = state.load_delta();
        assert!((delta - 100.0).abs() < 1e-9, "Buy fill should add +100 to delta, got {}", delta);
    }

    #[test]
    fn process_sell_fill_updates_delta() {
        let state = Arc::new(AtomicPortfolioState::new(100_000_000.0));
        let listener = create_test_listener(state.clone());

        let frame = make_exec_report(b'2', 50.0);
        listener.process_frame_for_test(&frame);

        let delta = state.load_delta();
        assert!((delta - (-50.0)).abs() < 1e-9, "Sell fill should add -50 to delta, got {}", delta);
    }

    #[test]
    fn process_multiple_fills_accumulate() {
        let state = Arc::new(AtomicPortfolioState::new(100_000_000.0));
        let listener = create_test_listener(state.clone());

        // Buy 100
        listener.process_frame_for_test(&make_exec_report(b'1', 100.0));
        // Buy 50
        listener.process_frame_for_test(&make_exec_report(b'1', 50.0));
        // Sell 75
        listener.process_frame_for_test(&make_exec_report(b'2', 75.0));

        let delta = state.load_delta();
        assert!((delta - 75.0).abs() < 1e-9, "Net delta should be 75, got {}", delta);
        assert_eq!(listener.fill_count(), 3);
    }

    #[test]
    fn non_execution_report_ignored() {
        let state = Arc::new(AtomicPortfolioState::new(100_000_000.0));
        let listener = create_test_listener(state.clone());

        // MsgType = '0' (Heartbeat), should be ignored
        let frame = b"8=FIX.4.4\x0135=0\x0132=100\x01";
        listener.process_frame_for_test(frame);

        assert!((state.load_delta() - 0.0).abs() < 1e-9, "Heartbeat should not update delta");
    }

    #[test]
    fn partial_fill_processed() {
        let state = Arc::new(AtomicPortfolioState::new(100_000_000.0));
        let listener = create_test_listener(state.clone());

        // ExecType '1' = Partial Fill
        let frame = b"8=FIX.4.4\x0135=8\x01150=1\x0154=1\x0132=25.5\x0110=000\x01";
        listener.process_frame_for_test(frame);

        assert!((state.load_delta() - 25.5).abs() < 1e-9, "Partial fill should update delta");
    }

    /// Helper to create a test listener with a dummy TCP stream.
    /// We create a loopback connection for testing.
    fn create_test_listener(state: Arc<AtomicPortfolioState>) -> RawDropCopyListener {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _handle = std::thread::spawn(move || {
            let _ = listener.accept();
        });
        let stream = TcpStream::connect(addr).unwrap();
        RawDropCopyListener {
            stream,
            portfolio_state: state,
            fill_count: AtomicU64::new(0),
        }
    }
}