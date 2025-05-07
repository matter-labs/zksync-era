use anyhow::Context as _;
use zksync_config::configs::contracts::chain::L2Contracts;
use zksync_dal::di::{MasterPool, PoolResource};
use zksync_eth_client::di::SettlementLayerContractsResource;
use zksync_node_framework::{
    resource::Unique,
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_types::L2_ASSET_ROUTER_ADDRESS;

use super::resources::OutputHandlerResource;
use crate::{
    io::seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, L2BlockSealerTask, OutputHandler,
    StateKeeperPersistence, TreeWritesPersistence,
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
    l2_block_seal_queue_capacity: usize,
    /// Whether transactions should be pre-inserted to DB.
    /// Should be set to `true` for EN's IO as EN doesn't store transactions in DB
    /// before they are included into L2 blocks.
    pre_insert_txs: bool,
    /// Whether protective reads persistence is enabled.
    /// May be set to `false` for nodes that do not participate in the sequencing process (e.g. external nodes)
    /// or run `vm_runner_protective_reads` component.
    protective_reads_persistence_enabled: bool,
    l2_contracts: L2Contracts,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    // FIXME: inject sync_state as additional output handler
    pub contracts: SettlementLayerContractsResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    pub output_handler: OutputHandlerResource,
    #[context(task)]
    pub l2_block_sealer: L2BlockSealerTask,
}

impl OutputHandlerLayer {
    pub fn new(l2_block_seal_queue_capacity: usize, l2_contracts: L2Contracts) -> Self {
        Self {
            l2_block_seal_queue_capacity,
            l2_contracts,
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

        let l2_shared_bridge_addr = self.l2_contracts.shared_bridge_addr;
        let l2_legacy_shared_bridge_addr = if l2_shared_bridge_addr == L2_ASSET_ROUTER_ADDRESS {
            // System has migrated to `L2_ASSET_ROUTER_ADDRESS`, use legacy shared bridge address from main node.
            self.l2_contracts.legacy_shared_bridge_addr
        } else {
            // System hasn't migrated on `L2_ASSET_ROUTER_ADDRESS`, we can safely use `l2_shared_bridge_addr`.
            Some(l2_shared_bridge_addr)
        };

        let (mut persistence, l2_block_sealer) = StateKeeperPersistence::new(
            persistence_pool.clone(),
            l2_legacy_shared_bridge_addr,
            self.l2_block_seal_queue_capacity,
        )
        .await?;
        if self.pre_insert_txs {
            persistence = persistence.with_tx_insertion();
        }
        if !self.protective_reads_persistence_enabled {
            persistence = persistence.without_protective_reads();
        }

        let tree_writes_persistence = TreeWritesPersistence::new(persistence_pool);
        let output_handler = OutputHandler::new(Box::new(persistence))
            .with_handler(Box::new(tree_writes_persistence));
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
