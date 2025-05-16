//! Dependency injection for circuit breakers.

use std::sync::Arc;

use zksync_config::configs::chain::CircuitBreakerConfig;
use zksync_node_framework::{
    task::TaskKind, FromContext, IntoContext, Resource, StopReceiver, Task, TaskId, WiringError,
    WiringLayer,
};

use crate::{CircuitBreakerChecker, CircuitBreakers};

impl Resource for CircuitBreakers {
    fn name() -> String {
        "common/circuit_breakers".into()
    }
}

/// Wiring layer for circuit breaker checker
///
/// Expects other layers to insert different components' circuit breakers into
/// [`crate::CircuitBreakers`] collection using [`CircuitBreakersResource`].
/// The added task periodically runs checks for all inserted circuit breakers.
#[derive(Debug)]
pub struct CircuitBreakerCheckerLayer(pub CircuitBreakerConfig);

#[derive(Debug, FromContext)]
pub struct Input {
    #[context(default)]
    circuit_breakers: Arc<CircuitBreakers>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    circuit_breaker_checker: CircuitBreakerChecker,
}

#[async_trait::async_trait]
impl WiringLayer for CircuitBreakerCheckerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "circuit_breaker_checker_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let circuit_breaker_checker =
            CircuitBreakerChecker::new(input.circuit_breakers, self.0.sync_interval());

        Ok(Output {
            circuit_breaker_checker,
        })
    }
}

#[async_trait::async_trait]
impl Task for CircuitBreakerChecker {
    fn kind(&self) -> TaskKind {
        TaskKind::UnconstrainedTask
    }

    fn id(&self) -> TaskId {
        "circuit_breaker_checker".into()
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
