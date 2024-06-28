use std::sync::Arc;

use zksync_base_token_adjuster::BaseTokenFetcher;

use crate::{
    implementations::resources::{
        base_token_fetcher::BaseTokenFetcherResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// A layer that inserts a resource for [`BaseTokenFetcher`].
#[derive(Debug)]
pub struct BaseTokenFetcherLayer;

#[async_trait::async_trait]
impl WiringLayer for BaseTokenFetcherLayer {
    fn layer_name(&self) -> &'static str {
        "base_token_fetcher"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool_resource = context.get_resource::<PoolResource<MasterPool>>().await?;
        let master_pool = master_pool_resource.get().await?;

        let fetcher = BaseTokenFetcher {
            pool: Some(master_pool),
            latest_ratio: None,
        };

        context.insert_resource(BaseTokenFetcherResource(Arc::new(fetcher.clone())))?;
        context.add_task(Box::new(fetcher));

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for BaseTokenFetcher {
    fn id(&self) -> TaskId {
        "base_token_fetcher".into()
    }

    async fn run(mut self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await?;
        Ok(())
    }
}
