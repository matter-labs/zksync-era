use vise::{Gauge, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "vm_call_tracer")]
pub struct CallMetrics {
    /// Maximum call stack depth during the execution of the transaction.
    pub call_stack_depth: Gauge<usize>,
    /// Maximum number of near calls during the execution of the transaction.
    pub max_near_calls: Gauge<usize>,
}

#[vise::register]
pub static CALL_METRICS: vise::Global<CallMetrics> = vise::Global::new();
