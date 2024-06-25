use std::sync::Arc;

use anyhow::Context as _;
use zksync_config::configs::{
    chain::{MempoolConfig, StateKeeperConfig},
    wallets,
};
use zksync_state_keeper::{MempoolFetcher, MempoolGuard, MempoolIO, SequencerSealer};
use zksync_types::L2ChainId;

use crate::{
    implementations::resources::{
        fee_input::FeeInputResource,
        pools::{MasterPool, PoolResource},
        state_keeper::{ConditionalSealerResource, StateKeeperIOResource},
    },
    resource::Unique,
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `MempoolIO`, an IO part of state keeper used by the main node.
///
/// ## Requests resources
///
/// - `FeeInputResource`
/// - `PoolResource<MasterPool>`
///
/// - `AppHealthCheckResource` (adds a health check)
///
/// ## Adds resources
///
/// - `StateKeeperIOResource`
/// - `ConditionalSealerResource`
///
/// ## Adds tasks
///
/// - `MempoolFetcherTask`
/// - `TaskTypeName2`
#[derive(Debug)]
pub struct MempoolIOLayer {
    zksync_network_id: L2ChainId,
    state_keeper_config: StateKeeperConfig,
    mempool_config: MempoolConfig,
    wallets: wallets::StateKeeper,
}

impl MempoolIOLayer {
    pub fn new(
        zksync_network_id: L2ChainId,
        state_keeper_config: StateKeeperConfig,
        mempool_config: MempoolConfig,
        wallets: wallets::StateKeeper,
    ) -> Self {
        Self {
            zksync_network_id,
            state_keeper_config,
            mempool_config,
            wallets,
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
impl WiringLayer for MempoolIOLayer {
    fn layer_name(&self) -> &'static str {
        "mempool_io_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Fetch required resources.
        let batch_fee_input_provider = context.get_resource::<FeeInputResource>().await?.0;
        let master_pool = context.get_resource::<PoolResource<MasterPool>>().await?;

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
        context.add_task(Box::new(MempoolFetcherTask(mempool_fetcher)));

        // Create mempool IO resource.
        let mempool_db_pool = master_pool
            .get_singleton()
            .await
            .context("Get master pool")?;
        let io = MempoolIO::new(
            mempool_guard,
            batch_fee_input_provider,
            mempool_db_pool,
            &self.state_keeper_config,
            self.wallets.fee_account.address(),
            self.mempool_config.delay_interval(),
            self.zksync_network_id,
        )
        .await?;
        context.insert_resource(StateKeeperIOResource(Unique::new(Box::new(io))))?;

        // Create sealer.
        let sealer = SequencerSealer::new(self.state_keeper_config);
        context.insert_resource(ConditionalSealerResource(Arc::new(sealer)))?;

        Ok(())
    }
}

#[derive(Debug)]
struct MempoolFetcherTask(MempoolFetcher);

#[async_trait::async_trait]
impl Task for MempoolFetcherTask {
    fn id(&self) -> TaskId {
        "state_keeper/mempool_fetcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.run(stop_receiver.0).await
    }
}
