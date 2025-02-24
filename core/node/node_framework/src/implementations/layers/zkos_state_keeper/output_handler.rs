use anyhow::Context as _;
use zksync_node_framework_derive::FromContext;
use zksync_zkos_state_keeper::{
    io::{seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, BlockPersistenceTask},
    OutputHandler, StateKeeperPersistence,
};

use crate::{
    implementations::resources::{
        pools::{MasterPool, PoolResource},
        state_keeper::ZkOsOutputHandlerResource,
        sync_state::SyncStateResource,
    },
    resource::Unique,
    wiring_layer::{WiringError, WiringLayer},
    IntoContext, StopReceiver, Task, TaskId,
};

/// Wiring layer for the state keeper output handler.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `SyncStateResource` (optional)
///
/// ## Adds resources
///
/// - `OutputHandlerResource`
///
/// ## Adds tasks
///
/// - `L2BlockSealerTask`
#[derive(Debug)]
pub struct OutputHandlerLayer {
    /// Whether transactions should be pre-inserted to DB.
    /// Should be set to `true` for EN's IO as EN doesn't store transactions in DB
    /// before they are included into L2 blocks.
    pre_insert_txs: bool,
    l2_block_seal_queue_capacity: usize,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub sync_state: Option<SyncStateResource>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub output_handler: ZkOsOutputHandlerResource,
    #[context(task)]
    pub block_persistence: BlockPersistenceTask,
}

impl OutputHandlerLayer {
    pub fn new(l2_block_seal_queue_capacity: usize) -> Self {
        Self {
            pre_insert_txs: false,
            l2_block_seal_queue_capacity,
        }
    }

    pub fn with_pre_insert_txs(mut self, pre_insert_txs: bool) -> Self {
        self.pre_insert_txs = pre_insert_txs;
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for OutputHandlerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "zk_os_state_keeper_output_handler_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Create L2 block sealer task and output handler.
        // L2 Block sealing process is parallelized, so we have to provide enough pooled connections.
        let persistence_pool = input
            .master_pool
            .get_custom(L2BlockSealProcess::subtasks_len())
            .await
            .context("Get master pool")?;

        let (mut persistence, block_persistence) = StateKeeperPersistence::new(
            persistence_pool.clone(),
            self.l2_block_seal_queue_capacity,
        )
        .await?;
        if self.pre_insert_txs {
            persistence = persistence.with_tx_insertion();
        }

        let output_handler = OutputHandler::new(Box::new(persistence));
        // if let Some(sync_state) = input.sync_state {
        //     output_handler = output_handler.with_handler(Box::new(sync_state.0));
        // }
        let output_handler = ZkOsOutputHandlerResource(Unique::new(output_handler));

        Ok(Output {
            output_handler,
            block_persistence,
        })
    }
}

#[async_trait::async_trait]
impl Task for BlockPersistenceTask {
    fn id(&self) -> TaskId {
        "zk_os_state_keeper/block_persistence".into()
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Task will exit itself once sender is dropped.
        (*self).run().await
    }
}
