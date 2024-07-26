use zksync_circuit_breaker::CircuitBreakerChecker;
use zksync_config::configs::chain::CircuitBreakerConfig;

use crate::{
    implementations::resources::circuit_breakers::CircuitBreakersResource,
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for circuit breaker checker
///
/// Expects other layers to insert different components' circuit breakers into
/// [`zksync_circuit_breaker::CircuitBreakers`] collection using [`CircuitBreakersResource`].
/// The added task periodically runs checks for all inserted circuit breakers.
#[derive(Debug)]
pub struct CircuitBreakerCheckerLayer(pub CircuitBreakerConfig);

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    #[context(default)]
    pub circuit_breakers: CircuitBreakersResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub circuit_breaker_checker: CircuitBreakerChecker,
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
            CircuitBreakerChecker::new(input.circuit_breakers.breakers, self.0.sync_interval());

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
