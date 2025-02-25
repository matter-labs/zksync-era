use anyhow::Context as _;
use zksync_config::configs::{
    chain::{MempoolConfig, StateKeeperConfig},
    wallets,
};
use zksync_state_keeper::{MempoolFetcher, MempoolGuard};
use zksync_types::L2ChainId;
use zksync_zkos_state_keeper::MempoolIO;

use crate::{
    implementations::resources::{
        fee_input::SequencerFeeInputResource,
        pools::{MasterPool, PoolResource},
        state_keeper::ZkOsStateKeeperIOResource,
    },
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
    pub fee_input: SequencerFeeInputResource,
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    pub state_keeper_io: ZkOsStateKeeperIOResource,
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

        Ok(Output {
            state_keeper_io: io.into(),
            mempool_fetcher,
        })
    }
}
