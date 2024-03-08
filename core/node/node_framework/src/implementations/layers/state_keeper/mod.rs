use std::sync::Arc;

use anyhow::Context;
use zksync_core::state_keeper::{
    seal_criteria::ConditionalSealer, BatchExecutor, StateKeeperIO, ZkSyncStateKeeper,
};
use zksync_storage::RocksDB;

pub mod main_batch_executor;
pub mod mempool_io;

use crate::{
    implementations::resources::state_keeper::{
        BatchExecutorResource, ConditionalSealerResource, StateKeeperIOResource,
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

/// Requests:
/// - `StateKeeperIOResource`
/// - `BatchExecutorResource`
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
            .await?
            .0
            .take()
            .context("StateKeeperIO was provided but taken by some other task")?;
        let batch_executor_base = context
            .get_resource::<BatchExecutorResource>()
            .await?
            .0
            .take()
            .context("L1BatchExecutorBuilder was provided but taken by some other task")?;
        let sealer = context.get_resource::<ConditionalSealerResource>().await?.0;

        context.add_task(Box::new(StateKeeperTask {
            io,
            batch_executor_base,
            sealer,
        }));
        Ok(())
    }
}

#[derive(Debug)]
struct StateKeeperTask {
    io: Box<dyn StateKeeperIO>,
    batch_executor_base: Box<dyn BatchExecutor>,
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
        let result = state_keeper.run().await;

        // Wait for all the instances of RocksDB to be destroyed.
        tokio::task::spawn_blocking(RocksDB::await_rocksdb_termination)
            .await
            .unwrap();

        result
    }
}
