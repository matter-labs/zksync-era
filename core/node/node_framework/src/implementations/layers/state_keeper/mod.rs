use std::sync::Arc;

use zksync_core::state_keeper::{
    seal_criteria::ConditionalSealer, L1BatchExecutorBuilder, StateKeeperIO, ZkSyncStateKeeper,
};

pub mod mempool_io;

use crate::{
    implementations::resources::state_keeper::{
        ConditionalSealerResource, L1BatchExecutorBuilderResource, StateKeeperIOResource,
    },
    resource::Resource,
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

/// Requests:
/// - `StateKeeperIOResource`
/// - `L1BatchExecutorBuilderResource`
/// - `ConditionalSealerResource`
///
#[derive(Debug)]
pub struct StateKeeperLayer;

#[async_trait::async_trait]
impl WiringLayer for StateKeeperLayer {
    fn layer_name(&self) -> &'static str {
        "state_keeper_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let io = context
            .get_resource::<StateKeeperIOResource>()
            .await
            .ok_or(WiringError::ResourceLacking(
                StateKeeperIOResource::resource_id(),
            ))?;
        let batch_executor_base = context
            .get_resource::<L1BatchExecutorBuilderResource>()
            .await
            .ok_or(WiringError::ResourceLacking(
                L1BatchExecutorBuilderResource::resource_id(),
            ))?;
        let sealer = context
            .get_resource::<ConditionalSealerResource>()
            .await
            .ok_or(WiringError::ResourceLacking(
                ConditionalSealerResource::resource_id(),
            ))?;

        context.add_task(Box::new(StateKeeperTask {
            io: io.0.take().unwrap(), // TODO: Do not unwrap
            batch_executor_base: batch_executor_base.0.take().unwrap(), // TODO: Do not unwrap
            sealer: sealer.0,
        }));
        Ok(())
    }
}

struct StateKeeperTask {
    io: Box<dyn StateKeeperIO>,
    batch_executor_base: Box<dyn L1BatchExecutorBuilder>,
    sealer: Arc<dyn ConditionalSealer>,
}

#[async_trait::async_trait]
impl Task for StateKeeperTask {
    fn name(&self) -> &'static str {
        "state_keeper"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let state_keeper = ZkSyncStateKeeper::new(
            stop_receiver.0,
            self.io,
            self.batch_executor_base,
            self.sealer,
        );
        state_keeper.run().await
    }
}
