use tokio::sync::oneshot;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId, TaskKind},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
};

/// Wiring layer that changes the handling of SIGINT signal, preventing an immediate shutdown.
/// Instead, it would propagate the signal to the rest of the node, allowing it to shut down gracefully.
#[derive(Debug)]
pub struct SigintHandlerLayer;

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub task: SigintHandlerTask,
}

#[async_trait::async_trait]
impl WiringLayer for SigintHandlerLayer {
    type Input = ();
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "sigint_handler_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        Ok(Output {
            task: SigintHandlerTask,
        })
    }
}

#[derive(Debug)]
pub struct SigintHandlerTask;

#[async_trait::async_trait]
impl Task for SigintHandlerTask {
    fn kind(&self) -> TaskKind {
        // SIGINT may happen at any time, so we must handle it as soon as it happens.
        TaskKind::UnconstrainedTask
    }

    fn id(&self) -> TaskId {
        "sigint_handler".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
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

        // Wait for either SIGINT or a stop request.
        tokio::select! {
            _ = sigint_receiver => {
                tracing::info!("Received SIGINT signal");
            },
            _ = stop_receiver.0.changed() => {},
        };

        Ok(())
    }
}
