use zksync_base_token_price_fetcher::{BaseTokenPriceFetcher, BaseTokenPriceFetcherConfig};

use crate::{
    implementations::resources::pools::{PoolResource, ReplicaPool},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct BaseTokenPriceFetcherLayer(pub BaseTokenPriceFetcherConfig);

#[async_trait::async_trait]
impl WiringLayer for BaseTokenPriceFetcherLayer {
    fn layer_name(&self) -> &'static str {
        "base_token_price_fetcher_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let connection_pool = context
            .get_resource::<PoolResource<ReplicaPool>>()
            .await?
            .get()
            .await?;

        let base_token_price_fetcher = BaseTokenPriceFetcher::new(self.0, connection_pool);
        context.add_task(Box::new(BaseTokenPriceFetcherTask {
            base_token_price_fetcher,
        }));
        Ok(())
    }
}

#[derive(Debug)]
pub struct BaseTokenPriceFetcherTask {
    base_token_price_fetcher: BaseTokenPriceFetcher,
}

#[async_trait::async_trait]
impl Task for BaseTokenPriceFetcherTask {
    fn id(&self) -> TaskId {
        "base_token_price_fetcher".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.base_token_price_fetcher.run(stop_receiver.0).await
    }
}
