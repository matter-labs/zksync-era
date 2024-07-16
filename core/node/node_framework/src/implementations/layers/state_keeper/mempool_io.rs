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
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for `MempoolIO`, an IO part of state keeper used by the main node.
///
/// ## Requests resources
///
/// - `FeeInputResource`
/// - `PoolResource<MasterPool>`
///
/// ## Adds resources
///
/// - `StateKeeperIOResource`
/// - `ConditionalSealerResource`
///
/// ## Adds tasks
///
/// - `MempoolFetcherTask`
#[derive(Debug)]
pub struct MempoolIOLayer {
    zksync_network_id: L2ChainId,
    state_keeper_config: StateKeeperConfig,
    mempool_config: MempoolConfig,
    wallets: wallets::StateKeeper,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub fee_input: FeeInputResource,
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub state_keeper_io: StateKeeperIOResource,
    pub conditional_sealer: ConditionalSealerResource,
    #[context(task)]
    pub mempool_fetcher: MempoolFetcher,
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
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "mempool_io_layer"
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
        let io = MempoolIO::new(
            mempool_guard,
            batch_fee_input_provider,
            mempool_db_pool,
            &self.state_keeper_config,
            self.wallets.fee_account.address(),
            self.mempool_config.delay_interval(),
            self.zksync_network_id,
        )?;

        // Create sealer.
        let sealer = SequencerSealer::new(self.state_keeper_config);

        Ok(Output {
            state_keeper_io: io.into(),
            conditional_sealer: sealer.into(),
            mempool_fetcher,
        })
    }
}

#[async_trait::async_trait]
impl Task for MempoolFetcher {
    fn id(&self) -> TaskId {
        "state_keeper/mempool_fetcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
