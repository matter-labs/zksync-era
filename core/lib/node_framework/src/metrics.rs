use vise::{Counter, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "node_tasks")]
pub(crate) struct TaskMetrics {
    /// Number of times a certain task was polled.
    #[metrics(labels = ["task"])]
    pub poll_count: LabeledFamily<String, Counter>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<TaskMetrics> = vise::Global::new();
