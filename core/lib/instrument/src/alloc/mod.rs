//! Allocation instrumentation.

use std::{marker::PhantomData, ops, thread};

use self::metrics::{OP_METRICS, TASK_METRICS};

mod metrics;

#[derive(Debug, Clone, Copy, Default)]
struct AllocationStats {
    allocated: u64,
    deallocated: u64,
}

impl AllocationStats {
    #[cfg(feature = "jemalloc")]
    fn for_current_thread() -> Option<Self> {
        let allocated = tikv_jemalloc_ctl::thread::allocatedp::read()
            .expect("failed reading allocated stats for current thread")
            .get();
        let deallocated = tikv_jemalloc_ctl::thread::deallocatedp::read()
            .expect("failed reading deallocated stats for current thread")
            .get();
        Some(Self {
            allocated,
            deallocated,
        })
    }

    #[cfg(not(feature = "jemalloc"))]
    fn current() -> Option<Self> {
        None
    }
}

impl ops::Sub for AllocationStats {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            allocated: self.allocated - rhs.allocated,
            deallocated: self.deallocated - rhs.deallocated,
        }
    }
}

impl ops::AddAssign for AllocationStats {
    fn add_assign(&mut self, rhs: Self) {
        self.allocated += rhs.allocated;
        self.deallocated += rhs.deallocated;
    }
}

#[derive(Debug)]
enum AllocationGuardKind<'a> {
    Operation(&'a str),
    Task(&'a str),
    Accumulator(Option<&'a mut AllocationStats>),
}

/// Monitors (de)allocation while in scope.
///
/// This type is `!Send` and thus should only be used to monitor single-threaded / blocking routines.
/// It cannot be used in Tokio futures.
#[derive(Debug)]
#[must_use = "Observes (de)allocation stats on drop"]
pub struct AllocationGuard<'a> {
    kind: AllocationGuardKind<'a>,
    stats: Option<AllocationStats>,
    _not_send: PhantomData<*mut ()>,
}

impl<'a> AllocationGuard<'a> {
    fn new(kind: AllocationGuardKind<'a>) -> Self {
        Self {
            kind,
            stats: AllocationStats::for_current_thread(),
            _not_send: PhantomData,
        }
    }

    /// Creates an allocation guard for the specified operation. The operation name should be globally unique.
    pub fn for_operation(operation: &'a str) -> Self {
        Self::new(AllocationGuardKind::Operation(operation))
    }

    /// Creates an allocation guard for the specified long-running task.
    pub fn for_task(task: &'a str) -> Self {
        Self::new(AllocationGuardKind::Task(task))
    }
}

impl Drop for AllocationGuard<'_> {
    fn drop(&mut self) {
        let (Some(stats), Some(new_stats)) = (self.stats, AllocationStats::for_current_thread())
        else {
            return;
        };
        let diff = new_stats - stats;

        if !thread::panicking() {
            match &mut self.kind {
                AllocationGuardKind::Operation(op) => OP_METRICS.observe_op_stats(op, diff),
                AllocationGuardKind::Task(task) => TASK_METRICS.observe_task_increments(task, diff),
                AllocationGuardKind::Accumulator(acc) => {
                    if let Some(acc_stats) = acc.as_deref_mut() {
                        *acc_stats += diff;
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct AllocationAccumulator<'a> {
    operation: &'a str,
    stats: Option<AllocationStats>,
}

impl<'a> AllocationAccumulator<'a> {
    pub fn new(operation: &'a str) -> Self {
        Self {
            operation,
            stats: cfg!(feature = "jemalloc").then(AllocationStats::default),
        }
    }

    pub fn start(&mut self) -> AllocationGuard<'_> {
        AllocationGuard::new(AllocationGuardKind::Accumulator(self.stats.as_mut()))
    }
}

impl Drop for AllocationAccumulator<'_> {
    fn drop(&mut self) {
        let Some(stats) = self.stats else {
            return;
        };
        if !thread::panicking() {
            OP_METRICS.observe_op_stats(self.operation, stats);
        }
    }
}
