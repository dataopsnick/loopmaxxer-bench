//! NUMA-Aware Thread Pinning (Spec §28, §34)
//!
//! Pins hot-path threads to dedicated CPU cores to eliminate context-switch
//! jitter and L3 cache pollution from other processes. Uses the `core_affinity`
//! crate for cross-platform core pinning.

use core_affinity::CoreId;

/// Configuration for NUMA-aware thread pinning.
///
/// Specifies which CPU core(s) to pin the hot-path ingestion and
/// orchestration threads to.
#[derive(Debug, Clone)]
pub struct NumaConfig {
    /// Core ID for the ingestion (RX) thread.
    pub rx_core_id: usize,
    /// Core ID for the orchestration (processing) thread.
    pub processing_core_id: usize,
    /// Core ID for the drop-copy listener thread.
    pub dropcopy_core_id: usize,
    /// Whether to actually pin threads (false on macOS dev).
    pub enable_pinning: bool,
}

impl Default for NumaConfig {
    fn default() -> Self {
        Self {
            // Default to cores 2, 3, 4 (leave 0-1 for OS)
            rx_core_id: 2,
            processing_core_id: 3,
            dropcopy_core_id: 4,
            enable_pinning: true,
        }
    }
}

impl NumaConfig {
    /// Create a NUMA config for development (no pinning).
    pub fn dev() -> Self {
        Self {
            enable_pinning: false,
            ..Default::default()
        }
    }

    /// Create a NUMA config with explicit core IDs.
    pub fn with_cores(rx: usize, processing: usize, dropcopy: usize) -> Self {
        Self {
            rx_core_id: rx,
            processing_core_id: processing,
            dropcopy_core_id: dropcopy,
            enable_pinning: true,
        }
    }
}

/// Pin the current thread to a specific CPU core.
///
/// On Linux, this uses `pthread_setaffinity_np` via the `core_affinity` crate.
/// On macOS, this is a no-op (macOS does not support thread affinity in the
/// same way; `core_affinity` will attempt a best-effort policy hint).
///
/// Returns `true` if pinning succeeded, `false` otherwise.
pub fn pin_thread_to_core(core_id: usize) -> bool {
    let core_ids = match core_affinity::get_core_ids() {
        Some(ids) => ids,
        None => return false,
    };

    if core_id >= core_ids.len() {
        return false;
    }

    let target = CoreId { id: core_id };
    core_affinity::set_for_current(target)
}

/// Pin the current thread based on a `NumaConfig` role.
///
/// # Arguments
/// * `config` - The NUMA configuration specifying core assignments.
/// * `role` - The role of this thread (rx, processing, or dropcopy).
pub fn pin_thread_by_role(config: &NumaConfig, role: ThreadRole) -> bool {
    if !config.enable_pinning {
        return false;
    }

    let core_id = match role {
        ThreadRole::Rx => config.rx_core_id,
        ThreadRole::Processing => config.processing_core_id,
        ThreadRole::DropCopy => config.dropcopy_core_id,
    };

    pin_thread_to_core(core_id)
}

/// The role of a thread in the hot-path pipeline.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ThreadRole {
    /// Ingestion / RX thread (reads from NIC via EF_VI).
    Rx,
    /// Processing / orchestration thread (pricing, risk, quoting).
    Processing,
    /// Drop-copy listener thread (FIX fill confirmation).
    DropCopy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numa_config_default() {
        let config = NumaConfig::default();
        assert!(config.enable_pinning);
        assert_eq!(config.rx_core_id, 2);
        assert_eq!(config.processing_core_id, 3);
        assert_eq!(config.dropcopy_core_id, 4);
    }

    #[test]
    fn numa_config_dev() {
        let config = NumaConfig::dev();
        assert!(!config.enable_pinning);
    }

    #[test]
    fn numa_config_with_cores() {
        let config = NumaConfig::with_cores(10, 11, 12);
        assert_eq!(config.rx_core_id, 10);
        assert_eq!(config.processing_core_id, 11);
        assert_eq!(config.dropcopy_core_id, 12);
        assert!(config.enable_pinning);
    }

    #[test]
    fn pin_thread_to_invalid_core_returns_false() {
        // Try to pin to a very high core ID that doesn't exist
        let result = pin_thread_to_core(9999);
        assert!(!result, "Pinning to non-existent core should fail");
    }

    #[test]
    fn pin_thread_by_role_dev_disabled() {
        let config = NumaConfig::dev();
        let result = pin_thread_by_role(&config, ThreadRole::Rx);
        assert!(!result, "Pinning should be disabled in dev mode");
    }

    #[test]
    fn thread_role_equality() {
        assert_eq!(ThreadRole::Rx, ThreadRole::Rx);
        assert_ne!(ThreadRole::Rx, ThreadRole::Processing);
    }
}