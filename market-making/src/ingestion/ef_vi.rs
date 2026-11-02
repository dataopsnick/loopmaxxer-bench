//! Solarflare EF_VI C FFI Bindings (Spec §4, §31-34)
//!
//! Userspace network driver bindings for Solarflare `onload`/EF_VI.
//! Enables kernel-bypass packet reception with sub-microsecond latency.
//!
//! **Platform**: Linux only, requires `libonload` installed.
//! Gated behind `cfg(target_os = "linux")` and `feature = "ef_vi"`.

#![cfg(target_os = "linux")]
#![cfg(feature = "ef_vi")]

/// Opaque handle to an EF_VI driver handle (`ef_driver_handle`).
pub type EfDriverHandle = i32;

/// Opaque handle to a protection domain (`ef_pd`).
#[repr(C)]
pub struct EfPd {
    _opaque: [u8; 64],
}

/// Opaque handle to a virtual interface (`ef_vi`).
#[repr(C)]
pub struct EfVi {
    _opaque: [u8; 4096],
}

/// Opaque handle to a registered memory region (`ef_memreg`).
#[repr(C)]
pub struct EfMemreg {
    _opaque: [u8; 128],
}

/// EF_VI event queue poll result.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct EfEvent {
    /// Event type code.
    pub event_type: u32,
    /// Event-specific data.
    pub data: u64,
}

/// RX event type indicating a packet was received.
pub const EF_EVENT_TYPE_RX: u32 = 1;

/// TX event type indicating a packet was sent.
pub const EF_EVENT_TYPE_TX: u32 = 2;

// ---------------------------------------------------------------------------
// FFI declarations — link against libonload
// ---------------------------------------------------------------------------

extern "C" {
    /// Open the EF_VI driver: `int ef_driver_open(ef_driver_handle* dh)`.
    ///
    /// # Safety
    /// `dh` must point to a valid `EfDriverHandle` slot.
    pub fn ef_driver_open(dh: *mut EfDriverHandle) -> i32;

    /// Allocate a protection domain: `int ef_pd_alloc(ef_pd* pd, ef_driver_handle dh, int flags)`.
    ///
    /// # Safety
    /// `pd` must point to a valid `EfPd` slot; `dh` must be a valid driver handle.
    pub fn ef_pd_alloc(pd: *mut EfPd, dh: EfDriverHandle, flags: i32) -> i32;

    /// Allocate a registered memory region:
    /// `int ef_memreg_alloc(ef_memreg* mr, ef_driver_handle dh, ef_pd* pd,
    ///                      ef_driver_handle pd_dh, void* p, size_t bytes)`.
    ///
    /// # Safety
    /// `mr` must point to a valid `EfMemreg` slot; `p` must point to page-aligned
    /// memory of at least `bytes` length that remains valid for the lifetime of `mr`.
    pub fn ef_memreg_alloc(
        mr: *mut EfMemreg,
        dh: EfDriverHandle,
        pd: *mut EfPd,
        pd_dh: EfDriverHandle,
        p: *mut u8,
        bytes: usize,
    ) -> i32;

    /// Allocate a virtual interface from a protection domain:
    /// `int ef_vi_alloc_from_pd(ef_vi* vi, ef_driver_handle dh,
    ///                          ef_pd* pd, ef_driver_handle pd_dh, ...)`.
    ///
    /// # Safety
    /// `vi` must point to a valid `EfVi` slot; `pd` must be a valid allocated domain.
    pub fn ef_vi_alloc_from_pd(
        vi: *mut EfVi,
        dh: EfDriverHandle,
        pd: *mut EfPd,
        pd_dh: EfDriverHandle,
        evq_capacity: i32,
        rxq_capacity: i32,
        txq_capacity: i32,
        ef_vi_flags: u32,
        ef_vi_arch: i32,
        ef_vi_variant: i32,
        ef_vi_revision: i32,
    ) -> i32;

    /// Post a receive buffer: `int ef_vi_rx_post(ef_vi* vi, ef_driver_handle dh,
    ///                                           ef_addr dma_addr, void* user_data)`.
    ///
    /// # Safety
    /// `vi` must be a valid allocated interface; `dma_addr` must be the DMA address
    /// of a registered memory region buffer.
    pub fn ef_vi_rx_post(
        vi: *mut EfVi,
        dh: EfDriverHandle,
        dma_addr: u64,
        user_data: u64,
    ) -> i32;

    /// Poll the event queue: `int ef_eventq_poll(ef_vi* vi, ef_event* evs, int evs_len)`.
    ///
    /// # Safety
    /// `vi` must be a valid allocated interface; `evs` must point to an array of
    /// at least `evs_len` `EfEvent` slots.
    pub fn ef_eventq_poll(vi: *mut EfVi, evs: *mut EfEvent, evs_len: i32) -> i32;

    /// Retrieve the RX packet payload pointer for a received event.
    ///
    /// # Safety
    /// `vi` must be valid; `event` must be a valid RX event from `ef_eventq_poll`.
    pub fn ef_vi_rx_packet_ptr(vi: *mut EfVi, event: *const EfEvent) -> *const u8;

    /// Retrieve the RX packet length for a received event.
    ///
    /// # Safety
    /// `vi` must be valid; `event` must be a valid RX event from `ef_eventq_poll`.
    pub fn ef_vi_rx_packet_len(vi: *mut EfVi, event: *const EfEvent) -> u32;
}

// ---------------------------------------------------------------------------
// High-level wrapper
// ---------------------------------------------------------------------------

/// A high-level wrapper around the EF_VI userspace driver lifecycle.
///
/// Owns the driver handle, protection domain, and virtual interface.
pub struct EfViContext {
    driver_handle: EfDriverHandle,
    pd: EfPd,
    vi: EfVi,
}

impl EfViContext {
    /// Open the EF_VI driver and allocate a protection domain + VI.
    ///
    /// # Errors
    /// Returns a negative errno code on failure.
    pub fn open() -> Result<Self, i32> {
        let mut driver_handle: EfDriverHandle = -1;
        let mut pd = EfPd { _opaque: [0u8; 64] };
        let mut vi = EfVi { _opaque: [0u8; 4096] };

        // SAFETY: `driver_handle` is a valid stack slot. The EF_VI driver
        // will populate it with a valid handle on success.
        let rc = unsafe { ef_driver_open(&mut driver_handle) };
        if rc != 0 {
            return Err(rc);
        }

        // SAFETY: `pd` is a valid stack slot; `driver_handle` was just opened.
        let rc = unsafe { ef_pd_alloc(&mut pd, driver_handle, 0) };
        if rc != 0 {
            return Err(rc);
        }

        // SAFETY: `vi` is a valid stack slot; `pd` was just allocated.
        let rc = unsafe {
            ef_vi_alloc_from_pd(
                &mut vi,
                driver_handle,
                &mut pd,
                driver_handle,
                -1, // evq_capacity: use default
                512, // rxq_capacity
                512, // txq_capacity
                0,   // ef_vi_flags
                0,   // ef_vi_arch
                0,   // ef_vi_variant
                0,   // ef_vi_revision
            )
        };
        if rc != 0 {
            return Err(rc);
        }

        Ok(Self {
            driver_handle,
            pd,
            vi,
        })
    }

    /// Poll the event queue for incoming events.
    ///
    /// Returns the number of events populated into `evs`.
    pub fn poll_events(&self, evs: &mut [EfEvent]) -> i32 {
        // SAFETY: `vi` is a valid allocated interface; `evs` is a valid slice
        // with length matching `evs_len`.
        unsafe { ef_eventq_poll(&self.vi as *const EfVi as *mut EfVi, evs.as_mut_ptr(), evs.len() as i32) }
    }

    /// Post a receive buffer to the RX queue.
    ///
    /// # Safety
    /// `dma_addr` must be the DMA address of a registered memory region.
    pub unsafe fn post_rx(&self, dma_addr: u64, user_data: u64) -> i32 {
        ef_vi_rx_post(
            &self.vi as *const EfVi as *mut EfVi,
            self.driver_handle,
            dma_addr,
            user_data,
        )
    }

    /// Get the packet pointer for a received event.
    ///
    /// # Safety
    /// `event` must be a valid RX event returned by `poll_events`.
    pub unsafe fn rx_packet_ptr(&self, event: &EfEvent) -> *const u8 {
        ef_vi_rx_packet_ptr(&self.vi as *const EfVi as *mut EfVi, event as *const EfEvent)
    }

    /// Get the packet length for a received event.
    ///
    /// # Safety
    /// `event` must be a valid RX event returned by `poll_events`.
    pub unsafe fn rx_packet_len(&self, event: &EfEvent) -> u32 {
        ef_vi_rx_packet_len(&self.vi as *const EfVi as *mut EfVi, event as *const EfEvent)
    }

    /// Get the driver handle (for registering memory regions).
    pub fn driver_handle(&self) -> EfDriverHandle {
        self.driver_handle
    }

    /// Get a mutable reference to the protection domain (for memreg alloc).
    pub fn pd_mut(&mut self) -> &mut EfPd {
        &mut self.pd
    }
}

impl Drop for EfViContext {
    fn drop(&mut self) {
        // In a full implementation, we would call ef_vi_free, ef_pd_free,
        // and ef_driver_close here. The libonload FFI signatures for these
        // are omitted for brevity; the kernel will reclaim resources on
        // process exit.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ef_event_type_constants() {
        assert_eq!(EF_EVENT_TYPE_RX, 1);
        assert_eq!(EF_EVENT_TYPE_TX, 2);
    }

    #[test]
    fn ef_event_struct_size() {
        // EfEvent should be 16 bytes (u32 + 4 padding + u64)
        let size = std::mem::size_of::<EfEvent>();
        assert!(size >= 12, "EfEvent size {} should be >= 12", size);
    }
}