use zksync_circuit_breaker::CircuitBreakerChecker;
use zksync_config::configs::chain::CircuitBreakerConfig;

use crate::{
    implementations::resources::circuit_breakers::CircuitBreakersResource,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct CircuitBreakerCheckerLayer(pub CircuitBreakerConfig);

#[async_trait::async_trait]
impl WiringLayer for CircuitBreakerCheckerLayer {
    fn layer_name(&self) -> &'static str {
        "circuit_breaker_checker_layer"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let mut circuit_breaker_resource = node
            .get_resource_or_default::<CircuitBreakersResource>()
            .await;
        if let Some(lag_limit) = self.0.replication_lag_limit_sec {
            circuit_breaker_resource.set_replication_lag_limit_sec(lag_limit);
        }

        let circuit_breaker_checker =
            CircuitBreakerChecker::new(circuit_breaker_resource.breakers, self.0.sync_interval());

        // Create and insert task.
        let task = CircuitBreakerCheckerTask {
            circuit_breaker_checker,
        };

        node.add_task(Box::new(task));
        Ok(())
    }
}

#[derive(Debug)]
struct CircuitBreakerCheckerTask {
    circuit_breaker_checker: CircuitBreakerChecker,
}

#[async_trait::async_trait]
impl Task for CircuitBreakerCheckerTask {
    fn name(&self) -> &'static str {
        "circuit_breaker_checker"
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.circuit_breaker_checker.run(stop_receiver.0).await
    }
}
