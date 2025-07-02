use std::sync::Arc;

use anyhow::Context as _;
use zksync_config::configs::{
    chain::{MempoolConfig, StateKeeperConfig},
    wallets,
};
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_health_check::AppHealthCheck;
use zksync_node_fee_model::node::SequencerFeeInputResource;
use zksync_node_framework::{
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_shared_resources::{api::SyncState, contracts::L2ContractsResource};
use zksync_state_keeper::{
    node::{ConditionalSealerResource, StateKeeperIOResource},
    seal_criteria::ConditionalSealer,
    MempoolFetcher, MempoolGuard, MempoolIO, SequencerSealer,
};
use zksync_types::{commitment::PubdataType, L2ChainId};

use super::resources::ActionQueueSenderResource;
use crate::{leader_io::LeaderIO, ActionQueue, ExternalIO};

/// Wiring layer for `LeaderIO`, an IO part of state keeper used by the leader node.
#[derive(Debug)]
pub struct LeaderIOLayer {
    chain_id: L2ChainId,
    state_keeper_config: StateKeeperConfig,
    mempool_config: MempoolConfig,
    fee_account: wallets::AddressWallet,
    pubdata_type: PubdataType,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub app_health: Arc<AppHealthCheck>,
    pub fee_input: SequencerFeeInputResource,
    pub master_pool: PoolResource<MasterPool>,
    pub l2_contracts: L2ContractsResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    action_queue_sender: ActionQueueSenderResource,
    io: StateKeeperIOResource,
    sealer: ConditionalSealerResource,
    #[context(task)]
    pub mempool_fetcher: MempoolFetcher,
}

impl LeaderIOLayer {
    pub fn new(
        chain_id: L2ChainId,
        state_keeper_config: StateKeeperConfig,
        mempool_config: MempoolConfig,
        fee_account: wallets::AddressWallet,
        pubdata_type: PubdataType,
    ) -> Self {
        Self {
            chain_id,
            state_keeper_config,
            mempool_config,
            fee_account,
            pubdata_type,
        }
    }

    async fn build_mempool_guard(
        &self,
        master_pool: &PoolResource<MasterPool>,
    ) -> anyhow::Result<MempoolGuard> {
        let connection_pool = master_pool
            .get_singleton()
            .await
            .context("Get connection pool")?;
        let mut storage = connection_pool
            .connection()
            .await
            .context("Access storage to build mempool")?;
        let mempool = MempoolGuard::from_storage(&mut storage, self.mempool_config.capacity).await;
        mempool.register_metrics();
        Ok(mempool)
    }
}

#[async_trait::async_trait]
impl WiringLayer for LeaderIOLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "leader_io_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let batch_fee_input_provider = input.fee_input.0;
        let master_pool = input.master_pool;

        // Create mempool fetcher task.
        let mempool_guard = self.build_mempool_guard(&master_pool).await?;
        let mempool_fetcher_pool = master_pool
            .get_singleton()
            .await
            .context("Get master pool")?;
        let mempool_fetcher = MempoolFetcher::new(
            mempool_guard.clone(),
            batch_fee_input_provider.clone(),
            &self.mempool_config,
            mempool_fetcher_pool,
        );

        // Create mempool IO resource.
        let mempool_db_pool = master_pool
            .get_singleton()
            .await
            .context("Get master pool")?;
        let mempool_io = MempoolIO::new(
            mempool_guard,
            batch_fee_input_provider,
            mempool_db_pool,
            &self.state_keeper_config,
            self.fee_account.address(),
            self.mempool_config.delay_interval,
            self.chain_id,
            input.l2_contracts.0.da_validator_addr,
            self.pubdata_type,
        )?;

        // Create sealer.
        let sealer: Box<dyn ConditionalSealer> =
            Box::new(SequencerSealer::new(self.state_keeper_config));

        // Create `ActionQueueSender` resource.
        let (action_queue_sender, action_queue) = ActionQueue::new();

        // Create external IO resource.
        let io_pool = master_pool.get().await.context("Get pool")?;
        let external_io = ExternalIO::new(io_pool, action_queue, None, self.chain_id)
            .context("Failed initializing I/O for external node state keeper")?;

        let io = LeaderIO::new(mempool_io, external_io);

        Ok(Output {
            action_queue_sender: action_queue_sender.into(),
            io: io.into(),
            sealer: sealer.into(),
            mempool_fetcher,
        })
    }
}
