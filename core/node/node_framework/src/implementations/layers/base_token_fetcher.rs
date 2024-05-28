use zksync_base_token_fetcher::BaseTokenFetcher;
use zksync_config::configs::BaseTokenFetcherConfig;

use crate::{
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct BaseTokenFetcherLayer(pub BaseTokenFetcherConfig);

#[async_trait::async_trait]
impl WiringLayer for BaseTokenFetcherLayer {
    fn layer_name(&self) -> &'static str {
        "base_token_fetcher_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        context.add_task(Box::new(BaseTokenFetcherTask {
            base_token_fetcher_config: self.0,
        }));
        Ok(())
    }
}

#[derive(Debug)]
pub struct BaseTokenFetcherTask {
    base_token_fetcher_config: BaseTokenFetcherConfig,
}

#[async_trait::async_trait]
impl Task for BaseTokenFetcherTask {
    fn name(&self) -> &'static str {
        "base_token_fetcher"
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        BaseTokenFetcher::new(self.base_token_fetcher_config).await?;
        Ok(())
    }
}
