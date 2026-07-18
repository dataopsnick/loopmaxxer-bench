//! Mapped Columnar Writer (Spec §10)
//!
//! Memory-mapped columnar trade log writer. On Linux, uses `mmap` +
//! `O_DIRECT` for zero-syscall writes. On macOS/other platforms, falls
//! back to an in-memory buffer for development/testing.
//!
//! Columnar layout:
//! ```text
//! [ Time Column (u64)  | Price Column (f64) | Size Column (u32) ]
//! ```

/// A single trade record for the columnar writer.
#[derive(Debug, Clone, Copy)]
pub struct TradeRecord {
    pub timestamp_ns: u64,
    pub price: f64,
    pub size: u32,
}

/// Size of each record's columns: u64 + f64 + u32 = 20 bytes.
pub const RECORD_SIZE: usize = std::mem::size_of::<u64>()
    + std::mem::size_of::<f64>()
    + std::mem::size_of::<u32>();

/// Columnar memory-mapped trade log writer (Spec §10).
///
/// On Linux, uses `mmap` with `O_DIRECT` for zero-copy persistent logging.
/// On other platforms, uses an in-memory `Vec` fallback.
pub struct MappedColumnarWriter {
    #[cfg(target_os = "linux")]
    file_ptr: *mut u8,
    #[cfg(target_os = "linux")]
    file_size: usize,
    capacity: usize,
    cursor: usize,
    #[cfg(not(target_os = "linux"))]
    time_col: Vec<u64>,
    #[cfg(not(target_os = "linux"))]
    price_col: Vec<f64>,
    #[cfg(not(target_os = "linux"))]
    size_col: Vec<u32>,
}

impl MappedColumnarWriter {
    /// Create a new columnar writer.
    ///
    /// On Linux: opens the file with `O_DIRECT`, sets length, and `mmap`s it.
    /// On other platforms: allocates in-memory vectors.
    #[cfg(target_os = "linux")]
    pub fn new(path: &str, capacity: usize) -> Self {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::io::AsRawFd;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .custom_flags(libc::O_DIRECT)
            .open(path)
            .expect("Failed to open high speed persistent file");

        let file_size =
            capacity * (std::mem::size_of::<u64>() + std::mem::size_of::<f64>() + std::mem::size_of::<u32>());
        file.set_len(file_size as u64).expect("Truncate failed");

        let fd = file.as_raw_fd();

        // SAFETY: mmap syscall maps file into virtual memory. The fd is
        // valid and the file has been truncated to file_size.
        let map_addr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                file_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };

        if map_addr == libc::MAP_FAILED {
            panic!("Critical: Memory map mapping failure");
        }

        // Keep the file handle alive by leaking it — the mmap persists
        // and munmap in Drop handles cleanup.
        std::mem::forget(file);

        Self {
            file_ptr: map_addr as *mut u8,
            file_size,
            capacity,
            cursor: 0,
        }
    }

    /// Create a new in-memory columnar writer (non-Linux fallback).
    #[cfg(not(target_os = "linux"))]
    pub fn new(_path: &str, capacity: usize) -> Self {
        Self {
            capacity,
            cursor: 0,
            time_col: Vec::with_capacity(capacity),
            price_col: Vec::with_capacity(capacity),
            size_col: Vec::with_capacity(capacity),
        }
    }

    /// Append a trade record to the columnar log.
    ///
    /// On Linux: writes via `write_volatile` to the mmap'd region (zero syscall).
    /// On other platforms: pushes to in-memory vectors.
    #[cfg(target_os = "linux")]
    #[inline(always)]
    pub fn append_trade_record(&mut self, timestamp_ns: u64, price: f64, size: u32) {
        if self.cursor >= self.capacity {
            return; // Overflow protection
        }

        // SAFETY: The mmap'd region is valid for file_size bytes, and
        // cursor < capacity ensures we stay within bounds. write_volatile
        // prevents the compiler from reordering the write.
        unsafe {
            // Time column: first capacity * sizeof(u64) bytes
            let time_base = self.file_ptr as *mut u64;
            time_base.add(self.cursor).write_volatile(timestamp_ns);

            // Price column: next capacity * sizeof(f64) bytes
            let price_offset =
                (self.file_ptr as usize) + (self.capacity * std::mem::size_of::<u64>());
            let price_base = price_offset as *mut f64;
            price_base.add(self.cursor).write_volatile(price);

            // Size column: next capacity * sizeof(u32) bytes
            let size_offset =
                price_offset + (self.capacity * std::mem::size_of::<f64>());
            let size_base = size_offset as *mut u32;
            size_base.add(self.cursor).write_volatile(size);
        }

        self.cursor += 1;
    }

    /// Append a trade record (in-memory fallback).
    #[cfg(not(target_os = "linux"))]
    #[inline(always)]
    pub fn append_trade_record(&mut self, timestamp_ns: u64, price: f64, size: u32) {
        if self.cursor >= self.capacity {
            return; // Overflow protection
        }
        self.time_col.push(timestamp_ns);
        self.price_col.push(price);
        self.size_col.push(size);
        self.cursor += 1;
    }

    /// Read a trade record at the given index (for testing).
    #[cfg(target_os = "linux")]
    pub fn read_record(&self, index: usize) -> Option<TradeRecord> {
        if index >= self.cursor {
            return None;
        }

        // SAFETY: index < cursor <= capacity, so the read is in bounds.
        unsafe {
            let time_base = self.file_ptr as *const u64;
            let ts = time_base.add(index).read_volatile();

            let price_offset =
                (self.file_ptr as usize) + (self.capacity * std::mem::size_of::<u64>());
            let price_base = price_offset as *const f64;
            let price = price_base.add(index).read_volatile();

            let size_offset =
                price_offset + (self.capacity * std::mem::size_of::<f64>());
            let size_base = size_offset as *const u32;
            let size = size_base.add(index).read_volatile();

            Some(TradeRecord {
                timestamp_ns: ts,
                price,
                size,
            })
        }
    }

    /// Read a trade record at the given index (in-memory fallback).
    #[cfg(not(target_os = "linux"))]
    pub fn read_record(&self, index: usize) -> Option<TradeRecord> {
        if index >= self.cursor {
            return None;
        }
        Some(TradeRecord {
            timestamp_ns: self.time_col[index],
            price: self.price_col[index],
            size: self.size_col[index],
        })
    }

    /// Get the current number of records written.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.cursor
    }

    /// Check if the writer is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.cursor == 0
    }

    /// Get the capacity.
    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Check if the writer has overflowed (cursor >= capacity).
    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.cursor >= self.capacity
    }
}

impl Drop for MappedColumnarWriter {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: munmap releases the virtual memory mapping.
            // The pointer and size were obtained from mmap in new().
            unsafe {
                libc::munmap(self.file_ptr as *mut libc::c_void, self.file_size);
            }
        }
        // In-memory fallback: vectors are dropped automatically
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_read_records() {
        // Use a temp file path; on non-Linux this is in-memory
        let mut writer = MappedColumnarWriter::new("/tmp/test_trade_col.log", 100);

        writer.append_trade_record(1000, 150.25, 100);
        writer.append_trade_record(2000, 151.50, 200);
        writer.append_trade_record(3000, 149.75, 50);

        assert_eq!(writer.len(), 3);
        assert!(!writer.is_empty());

        let r0 = writer.read_record(0).unwrap();
        assert_eq!(r0.timestamp_ns, 1000);
        assert!((r0.price - 150.25).abs() < 1e-9);
        assert_eq!(r0.size, 100);

        let r1 = writer.read_record(1).unwrap();
        assert_eq!(r1.timestamp_ns, 2000);
        assert!((r1.price - 151.50).abs() < 1e-9);
        assert_eq!(r1.size, 200);

        let r2 = writer.read_record(2).unwrap();
        assert_eq!(r2.timestamp_ns, 3000);
        assert!((r2.price - 149.75).abs() < 1e-9);
        assert_eq!(r2.size, 50);
    }

    #[test]
    fn overflow_protection() {
        let mut writer = MappedColumnarWriter::new("/tmp/test_overflow.log", 2);

        writer.append_trade_record(1000, 150.0, 100);
        writer.append_trade_record(2000, 151.0, 200);
        writer.append_trade_record(3000, 152.0, 300); // Should be dropped

        assert_eq!(writer.len(), 2);
        assert!(writer.is_full());

        // The third record should not have been written
        let r2 = writer.read_record(2);
        assert!(r2.is_none());
    }

    #[test]
    fn empty_writer() {
        let writer = MappedColumnarWriter::new("/tmp/test_empty.log", 10);
        assert!(writer.is_empty());
        assert_eq!(writer.len(), 0);
        assert_eq!(writer.capacity(), 10);
    }

    #[test]
    fn read_out_of_bounds_returns_none() {
        let writer = MappedColumnarWriter::new("/tmp/test_oob.log", 10);
        assert!(writer.read_record(0).is_none());
        assert!(writer.read_record(100).is_none());
    }
}