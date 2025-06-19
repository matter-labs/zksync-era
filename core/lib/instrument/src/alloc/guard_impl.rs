use std::fmt;

use tikv_jemalloc_ctl::thread::ThreadLocal;

use super::metrics::{OP_METRICS, TASK_METRICS};

pub(super) struct AllocationGuardImpl {
    name: &'static str,
    allocated: (u64, ThreadLocal<u64>),
    deallocated: (u64, ThreadLocal<u64>),
}

impl fmt::Debug for AllocationGuardImpl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AllocationGuardImpl")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl AllocationGuardImpl {
    pub(super) fn new(name: &'static str) -> Self {
        let allocated = tikv_jemalloc_ctl::thread::allocatedp::read()
            .expect("failed reading allocated stats for current thread");
        let deallocated = tikv_jemalloc_ctl::thread::deallocatedp::read()
            .expect("failed reading deallocated stats for current thread");
        Self {
            name,
            allocated: (allocated.get(), allocated),
            deallocated: (deallocated.get(), deallocated),
        }
    }

    pub(super) fn observe_operation(&self) {
        let (alloc, dealloc) = self.increments();
        OP_METRICS.observe_op_stats(self.name, alloc, dealloc);
    }

    fn increments(&self) -> (u64, u64) {
        let new_allocated = self.allocated.1.get();
        let new_deallocated = self.deallocated.1.get();
        (
            new_allocated - self.allocated.0,
            new_deallocated - self.deallocated.0,
        )
    }

    pub(super) fn observe_task(&self) {
        let (alloc, dealloc) = self.increments();
        TASK_METRICS.observe_task_increments(self.name, alloc, dealloc);
    }
}

#[derive(Debug)]
pub(super) struct AllocationAccumulatorImpl {
    name: &'static str,
    allocated: u64,
    deallocated: u64,
}

impl AllocationAccumulatorImpl {
    pub(super) fn new(name: &'static str) -> Self {
        Self {
            name,
            allocated: 0,
            deallocated: 0,
        }
    }

    pub(super) fn accumulate(&mut self, guard: &AllocationGuardImpl) {
        let (allocated, deallocated) = guard.increments();
        self.allocated += allocated;
        self.deallocated += deallocated;
    }

    pub(super) fn observe(&self) {
        OP_METRICS.observe_op_stats(self.name, self.allocated, self.deallocated);
    }
}
