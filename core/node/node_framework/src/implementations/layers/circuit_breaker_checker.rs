use zksync_circuit_breaker::CircuitBreakerChecker;
use zksync_config::configs::chain::CircuitBreakerConfig;

use crate::{
    implementations::resources::circuit_breakers::CircuitBreakersResource,
    service::{ServiceContext, StopReceiver},
    task::{TaskId, UnconstrainedTask},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for circuit breaker checker
///
/// Expects other layers to insert different components' circuit breakers into
/// [`zksync_circuit_breaker::CircuitBreakers`] collection using [`CircuitBreakersResource`].
/// The added task periodically runs checks for all inserted circuit breakers.
///
/// ## Adds resources
/// - [`CircuitBreakersResource`]
///
/// ## Adds tasks
/// - [`CircuitBreakerCheckerTask`] (as [`UnconstrainedTask`])
#[derive(Debug)]
pub struct CircuitBreakerCheckerLayer(pub CircuitBreakerConfig);

#[async_trait::async_trait]
impl WiringLayer for CircuitBreakerCheckerLayer {
    fn layer_name(&self) -> &'static str {
        "circuit_breaker_checker_layer"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let circuit_breaker_resource = node
            .get_resource_or_default::<CircuitBreakersResource>()
            .await;

        let circuit_breaker_checker =
            CircuitBreakerChecker::new(circuit_breaker_resource.breakers, self.0.sync_interval());

        // Create and insert task.
        let task = CircuitBreakerCheckerTask {
            circuit_breaker_checker,
        };

        node.add_unconstrained_task(Box::new(task));
        Ok(())
    }
}

#[derive(Debug)]
struct CircuitBreakerCheckerTask {
    circuit_breaker_checker: CircuitBreakerChecker,
}

#[async_trait::async_trait]
impl UnconstrainedTask for CircuitBreakerCheckerTask {
    fn id(&self) -> TaskId {
        "circuit_breaker_checker".into()
    }

    async fn run_unconstrained(
        mut self: Box<Self>,
        stop_receiver: StopReceiver,
    ) -> anyhow::Result<()> {
        self.circuit_breaker_checker.run(stop_receiver.0).await
    }
}
