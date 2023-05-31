use tokio::sync::watch;
use zksync_health_check::{CheckHealth, CheckHealthStatus};

use super::{MetadataCalculatorMode, MetadataCalculatorStatus};

/// HealthCheck used to verify if the tree(MetadataCalculator) is ready.
/// This guarantees that we mark a tree as ready only when it can start processing blocks.
/// Used in the /health endpoint
#[derive(Clone, Debug)]
pub struct TreeHealthCheck {
    receiver: watch::Receiver<MetadataCalculatorStatus>,
    tree_mode: MetadataCalculatorMode,
}

impl TreeHealthCheck {
    pub(super) fn new(
        receiver: watch::Receiver<MetadataCalculatorStatus>,
        tree_mode: MetadataCalculatorMode,
    ) -> TreeHealthCheck {
        TreeHealthCheck {
            receiver,
            tree_mode,
        }
    }
}

impl CheckHealth for TreeHealthCheck {
    fn check_health(&self) -> CheckHealthStatus {
        match *self.receiver.borrow() {
            MetadataCalculatorStatus::Ready => CheckHealthStatus::Ready,
            MetadataCalculatorStatus::NotReady => CheckHealthStatus::NotReady(format!(
                "{} tree is not ready",
                self.tree_mode.as_tag()
            )),
        }
    }
}
