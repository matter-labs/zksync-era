use std::sync::Arc;

use zksync_base_token_adjuster::DBBaseTokenRatioProvider;

use crate::{
    implementations::resources::{
        base_token_ratio_provider::BaseTokenRatioProviderResource,
        pools::{PoolResource, ReplicaPool},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `BaseTokenRatioProvider`
///
/// Responsible for serving the latest ETH<->BaseToken conversion ratio. This layer is only wired if
/// the base token is not ETH. If wired, this layer inserts the BaseTokenRatioProviderResource and kicks
/// off a task to poll the DB for the latest ratio and cache it.
///
/// If the base token is ETH, a default, no-op impl of the BaseTokenRatioProviderResource is used by other
/// layers to always return a conversion ratio of 1.
///
/// ## Requests resources
///
/// - `PoolResource<ReplicaPool>`
///
/// ## Adds tasks
///
/// - `BaseTokenRatioProvider`
#[derive(Debug)]
pub struct BaseTokenRatioProviderLayer;

#[async_trait::async_trait]
impl WiringLayer for BaseTokenRatioProviderLayer {
    fn layer_name(&self) -> &'static str {
        "base_token_ratio_provider"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let replica_pool_resource = context.get_resource::<PoolResource<ReplicaPool>>()?;
        let replica_pool = replica_pool_resource.get().await.unwrap();

        let ratio_provider = DBBaseTokenRatioProvider::new(replica_pool).await?;

        context.insert_resource(BaseTokenRatioProviderResource(Arc::new(
            ratio_provider.clone(),
        )))?;
        context.add_task(ratio_provider);

        Ok(())
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
