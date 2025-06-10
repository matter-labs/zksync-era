//! Allocation instrumentation.

use std::fmt;

use self::guard_impl::AllocationGuardImpl;

#[cfg(feature = "jemalloc")]
mod guard_impl;
#[cfg(not(feature = "jemalloc"))]
#[path = "mock_guard_impl.rs"]
mod guard_impl;
#[cfg(feature = "jemalloc")]
mod metrics;

/// Monitors (de)allocation while in scope.
///
/// This type is `!Send` and thus should only be used to monitor single-threaded / blocking routines.
/// It cannot be used in Tokio futures.
#[must_use = "Observes (de)allocation stats on drop"]
pub struct AllocationGuard {
    inner: AllocationGuardImpl,
}

impl fmt::Debug for AllocationGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, formatter)
    }
}

impl AllocationGuard {
    /// Creates an allocation guard for the specified operation. The operation name should be globally unique.
    pub fn new(operation: &'static str) -> Self {
        Self {
            inner: AllocationGuardImpl::new(operation),
        }
    }
}
