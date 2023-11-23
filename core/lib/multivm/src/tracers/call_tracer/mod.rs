use crate::tracers::call_tracer::metrics::CALL_METRICS;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use zksync_types::vm_trace::Call;

mod metrics;
pub mod vm_latest;
pub mod vm_refunds_enhancement;
pub mod vm_virtual_blocks;

#[derive(Debug, Clone)]
pub struct CallTracer {
    stack: Vec<FarcallAndNearCallCount>,
    result: Arc<OnceCell<Vec<Call>>>,

    max_stack_depth: usize,
    max_near_calls: usize,
}

#[derive(Debug, Clone)]
struct FarcallAndNearCallCount {
    farcall: Call,
    near_calls_after: usize,
}

impl Drop for CallTracer {
    fn drop(&mut self) {
        CALL_METRICS.call_stack_depth.observe(self.max_stack_depth);
        CALL_METRICS.max_near_calls.observe(self.max_near_calls);
    }
}

impl CallTracer {
    pub fn new(result: Arc<OnceCell<Vec<Call>>>) -> Self {
        Self {
            stack: vec![],
            result,
            max_stack_depth: 0,
            max_near_calls: 0,
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

    fn push_call_and_update_stats(&mut self, call: FarcallAndNearCallCount) {
        let near_calls_after = call.near_calls_after;
        self.stack.push(call);

        self.max_stack_depth = self
            .max_stack_depth
            .max(self.stack.len() + near_calls_after);
        self.max_near_calls = self.max_near_calls.max(near_calls_after);
    }

    fn increase_near_call_count(&mut self) {
        if let Some(last) = self.stack.last_mut() {
            last.near_calls_after += 1;

            let new_value = last.near_calls_after;

            self.max_near_calls = self.max_near_calls.max(new_value);
            self.max_stack_depth = self.max_stack_depth.max(self.stack.len() + new_value);
        }
    }
}
