//! Userspace Ingestion Driver (Spec §4, §16)
//!
//! Polling loop that reads packets from the NIC (via EF_VI on Linux or
//! a synthetic feed on macOS) and enqueues them into a lock-free ring
//! buffer for the orchestration thread to process.

use crossbeam_queue::ArrayQueue;

use crate::ingestion::dma_buffer::DmaBufferPool;
use crate::ingestion::numa::{pin_thread_by_role, NumaConfig, ThreadRole};
use crate::ingestion::spider_stream::{SpiderStreamHeader, StockBookQuoteBody};

/// Maximum number of pending ticks in the lock-free ring buffer.
pub const TICK_QUEUE_CAPACITY: usize = 65_536;

/// A parsed market tick extracted from a DMA frame.
#[derive(Debug, Clone, Copy)]
pub struct IngestedTick {
    /// Nanosecond timestamp from the SpiderStream header.
    pub timestamp_ns: u64,
    /// Message type (SBE schema ID).
    pub message_type: u16,
    /// Bid price (0 if not applicable).
    pub bid_price: f64,
    /// Ask price (0 if not applicable).
    pub ask_price: f64,
    /// Bid size (0 if not applicable).
    pub bid_size: i32,
    /// Ask size (0 if not applicable).
    pub ask_size: i32,
}

/// The userspace ingestion driver.
///
/// On Linux with EF_VI, this polls the NIC event queue directly.
/// On macOS (or without EF_VI), it consumes from a synthetic feed
/// injected via `inject_synthetic_frame`.
pub struct UserspaceIngestionDriver {
    /// Lock-free ring buffer for passing ticks to the orchestrator.
    tick_queue: ArrayQueue<IngestedTick>,
    /// Pool of pre-allocated DMA frame buffers.
    buffer_pool: DmaBufferPool,
    /// NUMA configuration for thread pinning.
    numa_config: NumaConfig,
    /// Number of frames processed.
    frames_processed: u64,
    /// Number of frames dropped (queue full).
    frames_dropped: u64,
    /// Whether the driver is running.
    running: bool,
}

impl UserspaceIngestionDriver {
    /// Create a new ingestion driver with the given NUMA config.
    pub fn new(numa_config: NumaConfig) -> Self {
        Self {
            tick_queue: ArrayQueue::new(TICK_QUEUE_CAPACITY),
            buffer_pool: DmaBufferPool::new(256),
            numa_config,
            frames_processed: 0,
            frames_dropped: 0,
            running: false,
        }
    }

    /// Create a new driver with default (dev) NUMA config.
    pub fn new_dev() -> Self {
        Self::new(NumaConfig::dev())
    }

    /// Pin the current thread to the RX core.
    ///
    /// Should be called at the start of the RX thread before entering
    /// the polling loop.
    pub fn pin_thread(&self) -> bool {
        pin_thread_by_role(&self.numa_config, ThreadRole::Rx)
    }

    /// Start the polling loop.
    ///
    /// On Linux with EF_VI, this enters a busy-poll loop on the NIC.
    /// On macOS, this is a no-op (use `inject_synthetic_frame` instead).
    pub fn start(&mut self) {
        self.running = true;
        let _ = self.pin_thread();

        #[cfg(all(target_os = "linux", feature = "ef_vi"))]
        {
            self.run_ef_vi_loop();
        }

        #[cfg(not(all(target_os = "linux", feature = "ef_vi")))]
        {
            // macOS / no EF_VI: the loop is driven externally via
            // `inject_synthetic_frame`. Nothing to do here.
        }
    }

    /// Stop the polling loop.
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Check if the driver is running.
    #[inline(always)]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Inject a synthetic frame for testing / macOS development.
    ///
    /// Parses the frame as a SpiderStream message and enqueues the
    /// resulting tick into the ring buffer.
    pub fn inject_synthetic_frame(&mut self, frame: &[u8]) -> bool {
        if let Some(tick) = Self::parse_spider_stream_frame(frame) {
            self.frames_processed += 1;
            if self.tick_queue.push(tick).is_err() {
                self.frames_dropped += 1;
                return false;
            }
            true
        } else {
            false
        }
    }

    /// Parse a raw frame buffer as a SpiderStream message.
    ///
    /// Extracts the header and (if applicable) the stock book quote body.
    pub fn parse_spider_stream_frame(frame: &[u8]) -> Option<IngestedTick> {
        let header_size = SpiderStreamHeader::SIZE;
        if frame.len() < header_size {
            return None;
        }

        // SAFETY: We checked that `frame` has at least `header_size` bytes.
        // `SpiderStreamHeader` is `#[repr(C, packed)]` so any alignment is valid.
        // We use `read_unaligned` to safely read from an unaligned pointer.
        let header: SpiderStreamHeader = unsafe {
            std::ptr::read_unaligned(frame.as_ptr() as *const SpiderStreamHeader)
        };

        // After the header, there's a 12-byte symbol key, then the body
        let body_offset = header_size + 12;
        let body_size = StockBookQuoteBody::SIZE;

        if frame.len() < body_offset + body_size {
            // Not a stock book quote or frame too short
            return Some(IngestedTick {
                timestamp_ns: header.sent_time,
                message_type: header.message_type,
                bid_price: 0.0,
                ask_price: 0.0,
                bid_size: 0,
                ask_size: 0,
            });
        }

        // SAFETY: We checked that `frame` has enough bytes for the body.
        // `StockBookQuoteBody` is `#[repr(C, packed)]` so any alignment is valid.
        let body: StockBookQuoteBody = unsafe {
            std::ptr::read_unaligned(
                frame.as_ptr().add(body_offset) as *const StockBookQuoteBody
            )
        };

        Some(IngestedTick {
            timestamp_ns: header.sent_time,
            message_type: header.message_type,
            bid_price: body.bid_price,
            ask_price: body.ask_price,
            bid_size: body.bid_size,
            ask_size: body.ask_size,
        })
    }

    /// Dequeue the next tick from the ring buffer (non-blocking).
    ///
    /// Returns `None` if the queue is empty.
    #[inline(always)]
    pub fn pop_tick(&self) -> Option<IngestedTick> {
        self.tick_queue.pop()
    }

    /// Get the number of frames processed.
    #[inline(always)]
    pub fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    /// Get the number of frames dropped (queue was full).
    #[inline(always)]
    pub fn frames_dropped(&self) -> u64 {
        self.frames_dropped
    }

    /// Get the current queue depth (approximate).
    #[inline(always)]
    pub fn queue_depth(&self) -> usize {
        self.tick_queue.len()
    }

    /// Get a mutable reference to the buffer pool (for EF_VI buffer management).
    pub fn buffer_pool_mut(&mut self) -> &mut DmaBufferPool {
        &mut self.buffer_pool
    }

    /// Get a reference to the NUMA config.
    pub fn numa_config(&self) -> &NumaConfig {
        &self.numa_config
    }

    /// The EF_VI busy-poll loop (Linux + ef_vi feature only).
    #[cfg(all(target_os = "linux", feature = "ef_vi"))]
    fn run_ef_vi_loop(&mut self) {
        use crate::ingestion::ef_vi::{EfViContext, EfEvent, EF_EVENT_TYPE_RX};

        let ctx = match EfViContext::open() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[INGESTION] Failed to open EF_VI context: errno {}", e);
                return;
            }
        };

        let mut events = [EfEvent { event_type: 0, data: 0 }; 64];

        while self.running {
            let n = ctx.poll_events(&mut events);
            for i in 0..n as usize {
                let event = &events[i];
                if event.event_type == EF_EVENT_TYPE_RX {
                    // SAFETY: `event` is a valid RX event returned by `poll_events`.
                    let pkt_ptr = unsafe { ctx.rx_packet_ptr(event) };
                    let pkt_len = unsafe { ctx.rx_packet_len(event) };

                    if !pkt_ptr.is_null() && pkt_len > 0 {
                        // SAFETY: `pkt_ptr` is valid for `pkt_len` bytes (DMA buffer).
                        let frame = unsafe {
                            std::slice::from_raw_parts(pkt_ptr, pkt_len as usize)
                        };

                        if let Some(tick) = Self::parse_spider_stream_frame(frame) {
                            self.frames_processed += 1;
                            if self.tick_queue.push(tick).is_err() {
                                self.frames_dropped += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_creation_dev() {
        let driver = UserspaceIngestionDriver::new_dev();
        assert!(!driver.is_running());
        assert_eq!(driver.frames_processed(), 0);
        assert_eq!(driver.frames_dropped(), 0);
        assert_eq!(driver.queue_depth(), 0);
    }

    #[test]
    fn driver_start_stop() {
        let mut driver = UserspaceIngestionDriver::new_dev();
        driver.start();
        // On macOS (no ef_vi), start() doesn't enter a loop
        assert!(driver.is_running());
        driver.stop();
        assert!(!driver.is_running());
    }

    #[test]
    fn inject_and_pop_tick() {
        let mut driver = UserspaceIngestionDriver::new_dev();

        // Build a synthetic SpiderStream frame
        let header = SpiderStreamHeader {
            sys_environment: 1,
            message_type: 1050,
            source_id: 42,
            sequence_number: 1,
            sent_time: 123456789,
            message_length: 32,
            key_length: 12,
        };

        let body = StockBookQuoteBody {
            bid_price: 149.98,
            ask_price: 150.02,
            bid_size: 500,
            ask_size: 500,
        };

        let header_size = SpiderStreamHeader::SIZE;
        let body_size = StockBookQuoteBody::SIZE;
        let body_offset = header_size + 12;

        let mut frame = vec![0u8; body_offset + body_size];

        // Write header
        unsafe {
            std::ptr::write_unaligned(frame.as_mut_ptr() as *mut SpiderStreamHeader, header);
        }

        // Write body
        unsafe {
            std::ptr::write_unaligned(
                frame.as_mut_ptr().add(body_offset) as *mut StockBookQuoteBody,
                body,
            );
        }

        // Inject
        assert!(driver.inject_synthetic_frame(&frame));
        assert_eq!(driver.frames_processed(), 1);
        assert_eq!(driver.queue_depth(), 1);

        // Pop
        let tick = driver.pop_tick().expect("should have a tick");
        assert_eq!(tick.timestamp_ns, 123456789);
        assert_eq!(tick.message_type, 1050);
        assert!((tick.bid_price - 149.98).abs() < 1e-9);
        assert!((tick.ask_price - 150.02).abs() < 1e-9);
        assert_eq!(tick.bid_size, 500);
        assert_eq!(tick.ask_size, 500);

        assert_eq!(driver.queue_depth(), 0);
    }

    #[test]
    fn parse_short_frame_returns_none() {
        let short_frame = [0u8; 4];
        let result = UserspaceIngestionDriver::parse_spider_stream_frame(&short_frame);
        assert!(result.is_none());
    }

    #[test]
    fn parse_header_only_frame() {
        // Frame with just a header (no body)
        let header = SpiderStreamHeader {
            sys_environment: 1,
            message_type: 999,
            source_id: 1,
            sequence_number: 1,
            sent_time: 42,
            message_length: 0,
            key_length: 0,
        };

        let header_size = SpiderStreamHeader::SIZE;
        let mut frame = vec![0u8; header_size];
        unsafe {
            std::ptr::write_unaligned(frame.as_mut_ptr() as *mut SpiderStreamHeader, header);
        }

        let tick = UserspaceIngestionDriver::parse_spider_stream_frame(&frame)
            .expect("should parse header");
        assert_eq!(tick.timestamp_ns, 42);
        assert_eq!(tick.message_type, 999);
        assert_eq!(tick.bid_price, 0.0);
    }

    #[test]
    fn queue_overflow_drops() {
        let driver = UserspaceIngestionDriver::new_dev();

        // Fill the queue to capacity
        for i in 0..TICK_QUEUE_CAPACITY {
            let tick = IngestedTick {
                timestamp_ns: i as u64,
                message_type: 1050,
                bid_price: 100.0,
                ask_price: 101.0,
                bid_size: 100,
                ask_size: 100,
            };
            assert!(driver.tick_queue.push(tick).is_ok());
        }

        // Next push should fail (queue full)
        let tick = IngestedTick {
            timestamp_ns: 999999,
            message_type: 1050,
            bid_price: 100.0,
            ask_price: 101.0,
            bid_size: 100,
            ask_size: 100,
        };
        assert!(driver.tick_queue.push(tick).is_err());
    }
}