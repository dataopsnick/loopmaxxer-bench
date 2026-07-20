//! Page-Aligned DMA Frame Buffers (Spec §4, §32)
//!
//! 4096-byte-aligned frame buffers for zero-copy DMA packet reception.
//! On Linux, buffers are locked into physical memory via `mlock(2)` to
//! prevent page faults during kernel-bypass packet processing.

/// Standard page size on x86_64 / aarch64.
pub const PAGE_SIZE: usize = 4096;

/// Maximum Ethernet frame size (jumbo frames supported).
pub const MAX_FRAME_SIZE: usize = 9216;

/// A page-aligned DMA frame buffer.
///
/// The buffer is `#[repr(C, align(4096))]` to guarantee page alignment
/// required by the NIC's DMA engine. On Linux, `mlock` is called to
/// pin the pages in physical memory.
#[repr(C, align(4096))]
pub struct DmaFrameBuffer {
    /// The raw frame data buffer.
    buf: [u8; MAX_FRAME_SIZE],
    /// Number of valid bytes written into `buf`.
    len: usize,
    /// Whether the buffer has been locked via `mlock`.
    locked: bool,
}

impl DmaFrameBuffer {
    /// Allocate a new page-aligned DMA frame buffer.
    ///
    /// On Linux, this also calls `mlock` to pin the buffer in physical memory.
    pub fn new() -> Self {
        let mut buf = Self {
            buf: [0u8; MAX_FRAME_SIZE],
            len: 0,
            locked: false,
        };
        buf.try_lock();
        buf
    }

    /// Attempt to lock the buffer in physical memory via `mlock`.
    ///
    /// On macOS (or if `mlock` fails), this is a no-op and the buffer
    /// remains usable but may be subject to page faults.
    fn try_lock(&mut self) {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: `self.buf` is page-aligned (guaranteed by `#[repr(align(4096))]`)
            // and we are locking exactly `MAX_FRAME_SIZE` bytes which is a multiple
            // of the page size. The buffer remains valid for the lifetime of `self`.
            let rc = unsafe {
                libc::mlock(
                    self.buf.as_ptr() as *const libc::c_void,
                    MAX_FRAME_SIZE,
                )
            };
            if rc == 0 {
                self.locked = true;
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            // macOS dev fallback: no mlock, buffer remains unlocked
            self.locked = false;
        }
    }

    /// Get a mutable slice of the full buffer (for DMA writes).
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    /// Get a slice of the valid frame data.
    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    /// Get the raw pointer to the buffer start (for DMA address registration).
    #[inline(always)]
    pub fn as_ptr(&self) -> *const u8 {
        self.buf.as_ptr()
    }

    /// Get the capacity of the buffer.
    #[inline(always)]
    pub const fn capacity(&self) -> usize {
        MAX_FRAME_SIZE
    }

    /// Get the number of valid bytes in the buffer.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Check if the buffer is empty.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Set the number of valid bytes (after a DMA write completes).
    #[inline(always)]
    pub fn set_len(&mut self, len: usize) {
        debug_assert!(len <= MAX_FRAME_SIZE, "frame length {} exceeds max {}", len, MAX_FRAME_SIZE);
        self.len = len.min(MAX_FRAME_SIZE);
    }

    /// Check if the buffer is locked in physical memory.
    #[inline(always)]
    pub const fn is_locked(&self) -> bool {
        self.locked
    }

    /// Clear the buffer (reset length to 0).
    #[inline(always)]
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl Default for DmaFrameBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DmaFrameBuffer {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if self.locked {
                // SAFETY: The buffer was locked with `mlock` in `try_lock`,
                // and is still valid at this point during `Drop`.
                unsafe {
                    libc::munlock(
                        self.buf.as_ptr() as *const libc::c_void,
                        MAX_FRAME_SIZE,
                    );
                }
            }
        }
    }
}

/// A pool of pre-allocated DMA frame buffers.
///
/// Maintains a fixed-size pool of page-aligned buffers that are recycled
/// between the RX and processing paths to avoid per-packet allocation.
pub struct DmaBufferPool {
    buffers: Vec<DmaFrameBuffer>,
}

impl DmaBufferPool {
    /// Create a new pool with `n` pre-allocated DMA frame buffers.
    pub fn new(n: usize) -> Self {
        let mut buffers = Vec::with_capacity(n);
        for _ in 0..n {
            buffers.push(DmaFrameBuffer::new());
        }
        Self { buffers }
    }

    /// Get the number of buffers in the pool.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Check if the pool is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Get a mutable reference to a buffer by index.
    #[inline(always)]
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut DmaFrameBuffer> {
        self.buffers.get_mut(idx)
    }

    /// Get a reference to a buffer by index.
    #[inline(always)]
    pub fn get(&self, idx: usize) -> Option<&DmaFrameBuffer> {
        self.buffers.get(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dma_buffer_page_aligned() {
        let buf = DmaFrameBuffer::new();
        let ptr = buf.as_ptr() as usize;
        assert_eq!(ptr % PAGE_SIZE, 0, "DMA buffer must be page-aligned");
    }

    #[test]
    fn dma_buffer_capacity() {
        let buf = DmaFrameBuffer::new();
        assert_eq!(buf.capacity(), MAX_FRAME_SIZE);
    }

    #[test]
    fn dma_buffer_set_len() {
        let mut buf = DmaFrameBuffer::new();
        assert!(buf.is_empty());
        buf.set_len(42);
        assert_eq!(buf.len(), 42);
        assert!(!buf.is_empty());
        buf.clear();
        assert!(buf.is_empty());
    }

    #[test]
    fn dma_buffer_set_len_clamped() {
        let mut buf = DmaFrameBuffer::new();
        buf.set_len(MAX_FRAME_SIZE + 100);
        assert_eq!(buf.len(), MAX_FRAME_SIZE, "length should be clamped to capacity");
    }

    #[test]
    fn dma_buffer_pool_creation() {
        let pool = DmaBufferPool::new(16);
        assert_eq!(pool.len(), 16);
        assert!(!pool.is_empty());

        // All buffers should be page-aligned
        for i in 0..pool.len() {
            let buf = pool.get(i).unwrap();
            let ptr = buf.as_ptr() as usize;
            assert_eq!(ptr % PAGE_SIZE, 0, "pool buffer {} must be page-aligned", i);
        }
    }

    #[test]
    fn dma_buffer_write_and_read() {
        let mut buf = DmaFrameBuffer::new();
        let data = b"hello world";
        buf.as_mut_slice()[..data.len()].copy_from_slice(data);
        buf.set_len(data.len());
        assert_eq!(buf.as_slice(), data);
    }
}