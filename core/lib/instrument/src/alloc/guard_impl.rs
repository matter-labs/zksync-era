use std::{fmt, thread};

use tikv_jemalloc_ctl::thread::ThreadLocal;

use super::metrics::METRICS;

pub(super) struct AllocationGuardImpl {
    operation: &'static str,
    allocated: (u64, ThreadLocal<u64>),
    deallocated: (u64, ThreadLocal<u64>),
}

impl fmt::Debug for AllocationGuardImpl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AllocationGuardImpl")
            .field("operation", &self.operation)
            .finish_non_exhaustive()
    }
}

impl AllocationGuardImpl {
    pub(super) fn new(operation: &'static str) -> Self {
        let allocated = tikv_jemalloc_ctl::thread::allocatedp::read()
            .expect("failed reading allocated stats for current thread");
        let deallocated = tikv_jemalloc_ctl::thread::deallocatedp::read()
            .expect("failed reading deallocated stats for current thread");
        Self {
            operation,
            allocated: (allocated.get(), allocated),
            deallocated: (deallocated.get(), deallocated),
        }
    }

    fn observe(&self) {
        let new_allocated = self.allocated.1.get();
        let new_deallocated = self.deallocated.1.get();
        METRICS.observe_op_stats(
            self.operation,
            new_allocated - self.allocated.0,
            new_deallocated - self.deallocated.0,
        );
    }
}

impl Drop for AllocationGuardImpl {
    fn drop(&mut self) {
        if !thread::panicking() {
            self.observe();
        }
    }
}
