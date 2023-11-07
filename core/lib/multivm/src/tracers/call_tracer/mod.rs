use once_cell::sync::OnceCell;
use std::sync::Arc;
use zksync_types::vm_trace::Call;

pub mod vm_latest;
pub mod vm_refunds_enhancement;
pub mod vm_virtual_blocks;

#[derive(Debug, Clone)]
pub struct CallTracer {
    stack: Vec<FarcallAndNearCallCount>,
    result: Arc<OnceCell<Vec<Call>>>,
}

#[derive(Debug, Clone)]
struct FarcallAndNearCallCount {
    farcall: Call,
    near_calls_after: usize,
}

impl CallTracer {
    pub fn new(result: Arc<OnceCell<Vec<Call>>>) -> Self {
        Self {
            stack: vec![],
            result,
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
}
