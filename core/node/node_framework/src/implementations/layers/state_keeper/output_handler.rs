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

/// Wiring layer for the state keeper output handler.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `SyncStateResource` (optional)
/// - `AppHealthCheckResource` (adds a health check)
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
    /// Must be `true` for any node that maintains a full Merkle Tree (e.g. any instance of main node).
    /// May be set to `false` for nodes that do not participate in the sequencing process (e.g. external nodes).
    protective_reads_persistence_enabled: bool,
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
            protective_reads_persistence_enabled: true,
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
    fn layer_name(&self) -> &'static str {
        "state_keeper_output_handler_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Fetch required resources.
        let master_pool = context.get_resource::<PoolResource<MasterPool>>()?;
        // Use `SyncState` if provided.
        let sync_state = match context.get_resource::<SyncStateResource>() {
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
            // **Important:** Disabling protective reads persistence is only sound if the node will never
            // run a full Merkle tree OR an accompanying protective-reads-writer is being run.
            tracing::warn!("Disabling persisting protective reads; this should be safe, but is considered an experimental option at the moment");
            persistence = persistence.without_protective_reads();
        }

        let tree_writes_persistence = TreeWritesPersistence::new(persistence_pool);
        let mut output_handler = OutputHandler::new(Box::new(persistence))
            .with_handler(Box::new(tree_writes_persistence));
        if let Some(sync_state) = sync_state {
            output_handler = output_handler.with_handler(Box::new(sync_state));
        }
        context.insert_resource(OutputHandlerResource(Unique::new(output_handler)))?;
        context.add_task(L2BlockSealerTask(l2_block_sealer));

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
