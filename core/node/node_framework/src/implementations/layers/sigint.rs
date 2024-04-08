use tokio::sync::oneshot;

use crate::{
    service::{ServiceContext, StopReceiver},
    task::UnconstrainedTask,
    wiring_layer::{WiringError, WiringLayer},
};

/// Layer that changes the handling of SIGINT signal, preventing an immediate shutdown.
/// Instead, it would propagate the signal to the rest of the node, allowing it to shut down gracefully.
#[derive(Debug)]
pub struct SigintHandlerLayer;

#[async_trait::async_trait]
impl WiringLayer for SigintHandlerLayer {
    fn layer_name(&self) -> &'static str {
        "sigint_handler_layer"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        // SIGINT may happen at any time, so we must handle it as soon as it happens.
        node.add_unconstrained_task(Box::new(SigintHandlerTask));
        Ok(())
    }
}

#[derive(Debug)]
struct SigintHandlerTask;

#[async_trait::async_trait]
impl UnconstrainedTask for SigintHandlerTask {
    fn name(&self) -> &'static str {
        "sigint_handler"
    }

    async fn run_unconstrained(
        self: Box<Self>,
        mut stop_receiver: StopReceiver,
    ) -> anyhow::Result<()> {
        let (sigint_sender, sigint_receiver) = oneshot::channel();
        let mut sigint_sender = Some(sigint_sender); // Has to be done this way since `set_handler` requires `FnMut`.
        ctrlc::set_handler(move || {
            if let Some(sigint_sender) = sigint_sender.take() {
                sigint_sender.send(()).ok();
                // ^ The send fails if `sigint_receiver` is dropped. We're OK with this,
                // since at this point the node should be stopping anyway, or is not interested
                // in listening to interrupt signals.
            }
        })
        .expect("Error setting Ctrl+C handler");

        // Wait for either SIGINT or stop signal.
        tokio::select! {
            _ = sigint_receiver => {},
            _ = stop_receiver.0.changed() => {},
        };

        Ok(())
    }
}
