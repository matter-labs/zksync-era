use anyhow::Context as _;
use zksync_node_framework_derive::FromContext;
use zksync_state_keeper::{
    io::seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, L2BlockSealerTask, OutputHandler,
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
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    IntoContext,
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
    l2_shared_bridge_addr: Address,
    l2_block_seal_queue_capacity: usize,
    l2_native_token_vault_proxy_addr: Address,
    /// Whether transactions should be pre-inserted to DB.
    /// Should be set to `true` for EN's IO as EN doesn't store transactions in DB
    /// before they are included into L2 blocks.
    pre_insert_txs: bool,
    /// Whether protective reads persistence is enabled.
    /// May be set to `false` for nodes that do not participate in the sequencing process (e.g. external nodes)
    /// or run `vm_runner_protective_reads` component.
    protective_reads_persistence_enabled: bool,
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
    pub output_handler: OutputHandlerResource,
    #[context(task)]
    pub l2_block_sealer: L2BlockSealerTask,
}

impl OutputHandlerLayer {
    pub fn new(
        l2_shared_bridge_addr: Address,
        l2_native_token_vault_proxy_addr: Address,
        l2_block_seal_queue_capacity: usize,
    ) -> Self {
        Self {
            l2_shared_bridge_addr,
            l2_block_seal_queue_capacity,
            l2_native_token_vault_proxy_addr,
            pre_insert_txs: false,
            protective_reads_persistence_enabled: false,
        }
    }

    pub fn with_pre_insert_txs(mut self, pre_insert_txs: bool) -> Self {
        self.pre_insert_txs = pre_insert_txs;
        self
    }

    pub fn with_protective_reads_persistence_enabled(
        mut self,
        protective_reads_persistence_enabled: bool,
    ) -> Self {
        self.protective_reads_persistence_enabled = protective_reads_persistence_enabled;
        self
    }
}

#[async_trait::async_trait]
impl WiringLayer for OutputHandlerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "state_keeper_output_handler_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Create L2 block sealer task and output handler.
        // L2 Block sealing process is parallelized, so we have to provide enough pooled connections.
        let persistence_pool = input
            .master_pool
            .get_custom(L2BlockSealProcess::subtasks_len())
            .await
            .context("Get master pool")?;
        let (mut persistence, l2_block_sealer) = StateKeeperPersistence::new(
            persistence_pool.clone(),
            self.l2_shared_bridge_addr,
            self.l2_native_token_vault_proxy_addr,
            self.l2_block_seal_queue_capacity,
        );
        if self.pre_insert_txs {
            persistence = persistence.with_tx_insertion();
        }
        if !self.protective_reads_persistence_enabled {
            persistence = persistence.without_protective_reads();
        }

        let tree_writes_persistence = TreeWritesPersistence::new(persistence_pool);
        let mut output_handler = OutputHandler::new(Box::new(persistence))
            .with_handler(Box::new(tree_writes_persistence));
        if let Some(sync_state) = input.sync_state {
            output_handler = output_handler.with_handler(Box::new(sync_state.0));
        }
        let output_handler = OutputHandlerResource(Unique::new(output_handler));

        Ok(Output {
            output_handler,
            l2_block_sealer,
        })
    }
}

#[async_trait::async_trait]
impl Task for L2BlockSealerTask {
    fn id(&self) -> TaskId {
        "state_keeper/l2_block_sealer".into()
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        // Miniblock sealer will exit itself once sender is dropped.
        (*self).run().await
    }
}
