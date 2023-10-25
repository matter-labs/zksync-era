use zksync_types::vm_trace::Call;

pub mod vm_latest;
pub mod vm_virtual_blocks;

#[derive(Debug, Clone)]
pub struct CallTracer {
    stack: Vec<FarcallAndNearCallCount>,
}
#[derive(Debug, Clone)]
struct FarcallAndNearCallCount {
    farcall: Call,
    near_calls_after: usize,
}

impl CallTracer {
    pub fn new() -> Self {
        Self { stack: vec![] }
    }

    fn extract_result(&mut self) -> Vec<Call> {
        std::mem::take(&mut self.stack)
            .into_iter()
            .map(|x| x.farcall)
            .collect()
    }
}
