use async_trait::async_trait;
use tokio::sync::watch;
use zksync_health_check::{CheckHealth, CheckHealthStatus};

/// HealthCheck used to verify if the Api is ready.
/// Used in the /health endpoint
#[derive(Clone, Debug)]
pub struct ApiHealthCheck {
    receiver: watch::Receiver<CheckHealthStatus>,
}

impl ApiHealthCheck {
    pub(super) fn new(receiver: watch::Receiver<CheckHealthStatus>) -> ApiHealthCheck {
        ApiHealthCheck { receiver }
    }
}

#[async_trait]
impl CheckHealth for ApiHealthCheck {
    async fn check_health(&self) -> CheckHealthStatus {
        match *self.receiver.borrow() {
            CheckHealthStatus::Ready => CheckHealthStatus::Ready,
            CheckHealthStatus::NotReady(ref error) => CheckHealthStatus::NotReady(error.clone()),
        }
    }
}
