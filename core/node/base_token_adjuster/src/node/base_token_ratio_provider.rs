use std::sync::Arc;

use zksync_config::BaseTokenAdjusterConfig;
use zksync_dal::node::{PoolResource, ReplicaPool};
use zksync_node_fee_model::BaseTokenRatioProvider;
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

use crate::DBBaseTokenRatioProvider;

/// Wiring layer for `BaseTokenRatioProvider`
///
/// Responsible for serving the latest ETH<->BaseToken conversion ratio. This layer is only wired if
/// the base token is not ETH. If wired, this layer inserts the BaseTokenRatioProviderResource and kicks
/// off a task to poll the DB for the latest ratio and cache it.
///
/// If the base token is ETH, a default, no-op impl of the BaseTokenRatioProviderResource is used by other
/// layers to always return a conversion ratio of 1.
#[derive(Debug)]
pub struct BaseTokenRatioProviderLayer {
    config: BaseTokenAdjusterConfig,
}

impl BaseTokenRatioProviderLayer {
    pub fn new(config: BaseTokenAdjusterConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, FromContext)]
pub struct Input {
    replica_pool: PoolResource<ReplicaPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    ratio_provider: Arc<dyn BaseTokenRatioProvider>,
    #[context(task)]
    ratio_provider_task: DBBaseTokenRatioProvider,
}

#[async_trait::async_trait]
impl WiringLayer for BaseTokenRatioProviderLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "base_token_ratio_provider"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let replica_pool = input.replica_pool.get().await.unwrap();

        let ratio_provider = DBBaseTokenRatioProvider::new(replica_pool, self.config).await?;
        // Cloning the provided preserves the internal state.
        Ok(Output {
            ratio_provider: Arc::new(ratio_provider.clone()),
            ratio_provider_task: ratio_provider,
        })
    }
}

#[async_trait::async_trait]
impl Task for DBBaseTokenRatioProvider {
    fn id(&self) -> TaskId {
        "base_token_ratio_provider".into()
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}
