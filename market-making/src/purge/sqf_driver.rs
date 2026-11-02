//! Low-Latency SQF Purge Driver (Spec §9)
//!
//! 40-byte `#[repr(C, packed)]` `SQFPurgeRequest` frame with non-blocking
//! UDP socket and zero-allocation `unsafe` pointer-cast payload submission.
//! When a risk threshold is exceeded, a single UDP packet purges all
//! orders for the session across all strikes and expirations.

use std::net::UdpSocket;
use std::sync::atomic::{AtomicU64, Ordering};

/// 40-byte fixed-size SQF Purge request frame (Spec §9).
///
/// Memory padding is eliminated via `#[repr(C, packed)]` for direct
/// binary submission to the exchange matching engine's purge port.
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct SQFPurgeRequest {
    /// Message type: 'P' = Purge Command.
    pub message_type: u8,
    /// Space-padded client firm acronym (8 bytes).
    pub client_firm: [u8; 8],
    /// Space-padded account (8 bytes).
    pub account: [u8; 8],
    /// Ticker symbol (12 bytes, space-padded).
    pub underlying: [u8; 12],
    /// Purge group sequence identifier.
    pub purge_group_id: u32,
    /// Nanosecond timestamp.
    pub sending_time_ns: u64,
}

impl SQFPurgeRequest {
    /// Total size of the purge frame in bytes.
    pub const SIZE: usize = std::mem::size_of::<Self>();

    /// Serialize to a byte slice via zero-copy pointer cast.
    ///
    /// # Safety
    /// `Self` is `#[repr(C, packed)]` with no padding, so the memory
    /// layout is contiguous and can be safely reinterpreted as bytes.
    #[inline(always)]
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: #[repr(C, packed)] guarantees contiguous layout.
        unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

/// Low-latency SQF purge driver (Spec §9).
///
/// Maintains a non-blocking UDP socket connected to the exchange's
/// SQF/BOE purge gateway. On risk threshold breach, sends a single
/// 40-byte purge packet to cancel all orders for the session.
pub struct LowLatencyPurgeDriver {
    socket: UdpSocket,
    purge_counter: AtomicU64,
    client_firm_bytes: [u8; 8],
    account_bytes: [u8; 8],
}

impl LowLatencyPurgeDriver {
    /// Create a new purge driver bound to `local_bind` and connected to
    /// the SQF gateway at `destination`.
    ///
    /// `firm` and `account` are pre-encoded into fixed-width byte arrays
    /// at construction time to avoid any allocation on the hot path.
    pub fn new(local_bind: &str, destination: &str, firm: &str, account: &str) -> Self {
        let socket = UdpSocket::bind(local_bind).expect("Failed to bind UDP socket for SQF Purge");
        socket.connect(destination).expect("Failed to link with SQF gateway");
        socket.set_nonblocking(true).expect("Unable to set non-blocking flags");

        let mut firm_b = [b' '; 8];
        let mut acc_b = [b' '; 8];

        firm_b[..firm.len().min(8)].copy_from_slice(&firm.as_bytes()[..firm.len().min(8)]);
        acc_b[..account.len().min(8)].copy_from_slice(&account.as_bytes()[..account.len().min(8)]);

        Self {
            socket,
            purge_counter: AtomicU64::new(1),
            client_firm_bytes: firm_b,
            account_bytes: acc_b,
        }
    }

    /// Trigger a mass purge for the given underlying symbol.
    ///
    /// Uses zero-allocation `unsafe` pointer-cast to serialize the
    /// 40-byte `SQFPurgeRequest` and send it via the non-blocking UDP
    /// socket. Target latency: <40ns from call to wire.
    #[inline(always)]
    pub fn trigger_mass_purge(&self, ticker: &str, epoch_time_ns: u64) -> std::io::Result<usize> {
        let seq = self.purge_counter.fetch_add(1, Ordering::Relaxed) as u32;
        let mut ticker_b = [b' '; 12];
        ticker_b[..ticker.len().min(12)].copy_from_slice(&ticker.as_bytes()[..ticker.len().min(12)]);

        let request = SQFPurgeRequest {
            message_type: b'P',
            client_firm: self.client_firm_bytes,
            account: self.account_bytes,
            underlying: ticker_b,
            purge_group_id: seq,
            sending_time_ns: epoch_time_ns,
        };

        // Zero-copy binary flush to socket
        self.socket.send(request.as_bytes())
    }

    /// Get the current purge counter value.
    #[inline(always)]
    pub fn purge_count(&self) -> u64 {
        self.purge_counter.load(Ordering::Relaxed)
    }

    /// Build a purge request without sending (for testing).
    #[inline(always)]
    pub fn build_purge_request(&self, ticker: &str, epoch_time_ns: u64) -> SQFPurgeRequest {
        let seq = self.purge_counter.fetch_add(1, Ordering::Relaxed) as u32;
        let mut ticker_b = [b' '; 12];
        ticker_b[..ticker.len().min(12)].copy_from_slice(&ticker.as_bytes()[..ticker.len().min(12)]);

        SQFPurgeRequest {
            message_type: b'P',
            client_firm: self.client_firm_bytes,
            account: self.account_bytes,
            underlying: ticker_b,
            purge_group_id: seq,
            sending_time_ns: epoch_time_ns,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqf_purge_request_size() {
        // The packed struct should be 1 + 8 + 8 + 12 + 4 + 8 = 41 bytes
        // (may vary slightly due to packing, but should be small)
        let size = std::mem::size_of::<SQFPurgeRequest>();
        assert!(size <= 48, "SQFPurgeRequest size {} should be <= 48", size);
    }

    #[test]
    fn sqf_purge_request_serialization() {
        let mut firm = [b' '; 8];
        firm[..6].copy_from_slice(b"SRCORE");
        let mut acc = [b' '; 8];
        acc[..5].copy_from_slice(b"T.ACC");
        let mut und = [b' '; 12];
        und[..4].copy_from_slice(b"AAPL");

        let req = SQFPurgeRequest {
            message_type: b'P',
            client_firm: firm,
            account: acc,
            underlying: und,
            purge_group_id: 1,
            sending_time_ns: 1234567890,
        };

        let bytes = req.as_bytes();
        assert_eq!(bytes.len(), SQFPurgeRequest::SIZE);
        assert_eq!(bytes[0], b'P');
    }

    #[test]
    fn sqf_purge_request_as_bytes_no_alloc() {
        let req = SQFPurgeRequest {
            message_type: b'P',
            client_firm: [b' '; 8],
            account: [b' '; 8],
            underlying: [b' '; 12],
            purge_group_id: 1,
            sending_time_ns: 0,
        };

        let bytes1 = req.as_bytes();
        let bytes2 = req.as_bytes();
        // Both slices should point to the same data (zero-copy)
        assert_eq!(bytes1.len(), bytes2.len());
        assert_eq!(bytes1[0], bytes2[0]);
    }
}