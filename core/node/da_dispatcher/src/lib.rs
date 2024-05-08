use zksync_config::da_dispatcher::ValidiumDAMode;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::data_availability::DataAvailabilityClient;

#[derive(Debug)]
pub struct DADispatcher {
    client: dyn DataAvailabilityClient,
    health_updater: HealthUpdater,
}

impl DADispatcher {
    pub fn new(mode: ValidiumDAMode) -> Self {
        Self {
            connector,
            health_updater: ReactiveHealthCheck::new("da_dispatcher").1,
        }
    }
}
