use std::sync::Arc;

use once_cell::sync::OnceCell;

use crate::{
    glue::tracers::IntoOldVmTracer, interface::Call, tracers::call_tracer::metrics::CALL_METRICS,
};

mod metrics;
pub mod vm_1_4_1;
pub mod vm_1_4_2;
pub mod vm_boojum_integration;
pub mod vm_latest;
pub mod vm_refunds_enhancement;
pub mod vm_virtual_blocks;

/// Maximum call input / output bytes the tracer reads out of VM memory per transaction;
pub const MAX_TRACE_DATA_BYTES_PER_TX: usize = 32 * 1_024 * 1_024;

#[derive(Debug, Clone)]
pub struct CallTracer {
    stack: Vec<FarcallAndNearCallCount>,
    finished_calls: Vec<Call>,

    result: Arc<OnceCell<Vec<Call>>>,

    max_stack_depth: usize,
    max_near_calls: usize,

    data_budget: DataBudget,
}

/// Budget of call input / output bytes a tracer is still allowed to retain.
#[derive(Debug, Clone)]
pub(crate) struct DataBudget {
    remaining: usize,
    dropped: usize,
}

impl Default for DataBudget {
    fn default() -> Self {
        Self::new(MAX_TRACE_DATA_BYTES_PER_TX)
    }
}

impl DataBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            remaining: limit,
            dropped: 0,
        }
    }

    /// Charges up to `len` bytes against the budget and returns how many of them may be retained.
    /// Callers must clamp the buffer they read out of VM memory to the returned length, so that
    /// oversized data is never allocated in the first place.
    pub(crate) fn take(&mut self, len: usize) -> usize {
        let allowed = len.min(self.remaining);
        self.remaining -= allowed;
        self.dropped += len - allowed;
        allowed
    }

    /// Reports the number of dropped bytes, if any. Called when a tracer is done.
    pub(crate) fn report(&self) {
        if self.dropped > 0 {
            CALL_METRICS.dropped_data_bytes.observe(self.dropped);
            tracing::debug!(
                dropped_bytes = self.dropped,
                "Call trace exceeded the retained-data budget of {MAX_TRACE_DATA_BYTES_PER_TX} B; \
                 call inputs / outputs past it were dropped"
            );
        }
    }
}

#[derive(Debug, Clone)]
struct FarcallAndNearCallCount {
    farcall: Call,
    near_calls_after: usize,
    stack_depth_on_prefix: usize,
}

impl Drop for CallTracer {
    fn drop(&mut self) {
        CALL_METRICS.call_stack_depth.observe(self.max_stack_depth);
        CALL_METRICS.max_near_calls.observe(self.max_near_calls);
        self.data_budget.report();
    }
}

impl CallTracer {
    pub fn new(result: Arc<OnceCell<Vec<Call>>>) -> Self {
        Self::with_data_budget(result, MAX_TRACE_DATA_BYTES_PER_TX)
    }

    /// Same as [`Self::new()`], with an explicit budget of retained input / output bytes.
    pub fn with_data_budget(result: Arc<OnceCell<Vec<Call>>>, data_budget: usize) -> Self {
        Self {
            stack: vec![],
            finished_calls: vec![],
            result,
            max_stack_depth: 0,
            max_near_calls: 0,
            data_budget: DataBudget::new(data_budget),
        }
    }

    fn extract_result(&mut self) -> Vec<Call> {
        std::mem::take(&mut self.stack)
            .into_iter()
            .map(|x| x.farcall)
            .collect()
    }

    fn store_result(&mut self) {
        let result = self.extract_result();
        let cell = self.result.as_ref();
        cell.set(result).unwrap();
    }

    fn push_call_and_update_stats(&mut self, farcall: Call, near_calls_after: usize) {
        let stack_depth = self
            .stack
            .last()
            .map(|x| x.stack_depth_on_prefix)
            .unwrap_or(0);

        let depth_on_prefix = stack_depth + 1 + near_calls_after;

        let call = FarcallAndNearCallCount {
            farcall,
            near_calls_after,
            stack_depth_on_prefix: depth_on_prefix,
        };

        self.stack.push(call);

        self.max_stack_depth = self.max_stack_depth.max(depth_on_prefix);
        self.max_near_calls = self.max_near_calls.max(near_calls_after);
    }

    fn increase_near_call_count(&mut self) {
        if let Some(last) = self.stack.last_mut() {
            last.near_calls_after += 1;
            last.stack_depth_on_prefix += 1;

            self.max_near_calls = self.max_near_calls.max(last.near_calls_after);
            self.max_stack_depth = self.max_stack_depth.max(last.stack_depth_on_prefix);
        }
    }
}

impl IntoOldVmTracer for CallTracer {
    fn old_tracer(&self) -> crate::tracers::old::OldTracers {
        crate::tracers::old::OldTracers::CallTracer(self.result.clone())
    }
}
