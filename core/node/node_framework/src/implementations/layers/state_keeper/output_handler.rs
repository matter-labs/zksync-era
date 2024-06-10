use anyhow::Context as _;

use zksync_state_keeper::{
    io::seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, OutputHandler,
    StateKeeperPersistence, TreeWritesPersistence,
};
use zksync_types::Address;

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        state_keeper::OutputHandlerResource,
        sync_state::SyncStateResource,
    },
    resource::Unique,
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct PersistenceLayer {
    l2_shared_bridge_addr: Address,
    l2_block_seal_queue_capacity: usize,
}

impl PersistenceLayer {
    pub fn new(l2_shared_bridge_addr: Address, l2_block_seal_queue_capacity: usize) -> Self {
        Self {
            l2_shared_bridge_addr,
            l2_block_seal_queue_capacity,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for PersistenceLayer {
    fn layer_name(&self) -> &'static str {
        "state_keeper_persistence_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Fetch required resources.
        let master_pool = context.get_resource::<PoolResource<MasterPool>>().await?;
        // Use `SyncState` if provided.
        let sync_state = match context.get_resource::<SyncStateResource>().await {
            Ok(sync_state) => Some(sync_state.0),
            Err(WiringError::ResourceLacking { .. }) => None,
            Err(err) => return Err(err),
        };

        // Create L2 block sealer task and output handler.
        // L2 Block sealing process is parallelized, so we have to provide enough pooled connections.
        let persistence_pool = master_pool
            .get_custom(L2BlockSealProcess::subtasks_len())
            .await
            .context("Get master pool")?;
        let (persistence, l2_block_sealer) = StateKeeperPersistence::new(
            persistence_pool.clone(),
            self.l2_shared_bridge_addr,
            self.l2_block_seal_queue_capacity,
        );
        let tree_writes_persistence = TreeWritesPersistence::new(persistence_pool);
        let mut output_handler = OutputHandler::new(Box::new(persistence))
            .with_handler(Box::new(tree_writes_persistence));
        if let Some(sync_state) = sync_state {
            output_handler = output_handler.with_handler(Box::new(sync_state));
        }
        context.insert_resource(OutputHandlerResource(Unique::new(output_handler)))?;
        context.add_task(Box::new(L2BlockSealerTask(l2_block_sealer)));

        Ok(())
    }
}

#[derive(Debug)]
struct L2BlockSealerTask(zksync_state_keeper::L2BlockSealerTask);

#[async_trait::async_trait]
impl Task for L2BlockSealerTask {
    fn id(&self) -> TaskId {
        "state_keeper/l2_block_sealer".into()
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Miniblock sealer will exit itself once sender is dropped.
        self.0.run().await
    }
}
