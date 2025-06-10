//! Allocation instrumentation.

use std::{fmt, thread};

use self::guard_impl::AllocationGuardImpl;

#[cfg(feature = "jemalloc")]
mod guard_impl;
#[cfg(not(feature = "jemalloc"))]
#[path = "mock_guard_impl.rs"]
mod guard_impl;
#[cfg(feature = "jemalloc")]
mod metrics;

#[derive(Debug, Clone, Copy)]
enum AllocationGuardKind {
    Operation,
    Task,
}

/// Monitors (de)allocation while in scope.
///
/// This type is `!Send` and thus should only be used to monitor single-threaded / blocking routines.
/// It cannot be used in Tokio futures.
#[must_use = "Observes (de)allocation stats on drop"]
pub struct AllocationGuard {
    inner: AllocationGuardImpl,
    kind: AllocationGuardKind,
}

impl fmt::Debug for AllocationGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, formatter)
    }
}

impl AllocationGuard {
    /// Creates an allocation guard for the specified operation. The operation name should be globally unique.
    pub fn for_operation(operation: &'static str) -> Self {
        Self {
            inner: AllocationGuardImpl::new(operation),
            kind: AllocationGuardKind::Operation,
        }
    }

    /// Creates an allocation guard for the specified long-running task.
    pub fn for_task(task: &'static str) -> Self {
        Self {
            inner: AllocationGuardImpl::new(task),
            kind: AllocationGuardKind::Task,
        }
    }
}

impl Drop for AllocationGuard {
    fn drop(&mut self) {
        if !thread::panicking() {
            match self.kind {
                AllocationGuardKind::Operation => self.inner.observe_operation(),
                AllocationGuardKind::Task => self.inner.observe_task(),
            }
        }
    }
}
