// add comments

use std::sync::Arc;

use futures::channel::oneshot;
use zksync_circuit_breaker::{CircuitBreakerChecker, CircuitBreakerError};
use zksync_config::configs::chain::CircuitBreakerConfig;

use crate::{
    implementations::resources::circuit_breaker_checker::CircuitBreakerCheckerResource,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct CircuitBreakerCheckerLayer {
    config: CircuitBreakerConfig,
}

#[async_trait::async_trait]
impl WiringLayer for CircuitBreakerCheckerLayer {
    fn layer_name(&self) -> &'static str {
        "circuit_breaker_checker_layer"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        let circuit_breaker_checker = Arc::new(CircuitBreakerChecker::new(None, &self.config));
        node.insert_resource(CircuitBreakerCheckerResource(
            circuit_breaker_checker.clone(),
        ))?;
        // maybe it has to be CircuitBreakerCheckerLayer field?
        let (cb_sender, _cb_receiver) = oneshot::channel();

        let task = CircuitBreakerCheckerTask {
            circuit_breaker_checker,
            cb_sender,
        };

        node.add_task(Box::new(task));
        Ok(())
    }
}

#[derive(Debug)]
struct CircuitBreakerCheckerTask {
    circuit_breaker_checker: Arc<CircuitBreakerChecker>,
    cb_sender: oneshot::Sender<CircuitBreakerError>,
}

#[async_trait::async_trait]
impl Task for CircuitBreakerCheckerTask {
    fn name(&self) -> &'static str {
        "circuit_breaker_checker"
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.circuit_breaker_checker
            .run(self.cb_sender, stop_receiver.0)
            .await
    }
}
