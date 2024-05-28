use zksync_config::configs::BaseTokenFetcherConfig;

use crate::{
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct DevConversionRateApiLayer(pub BaseTokenFetcherConfig);

#[async_trait::async_trait]
impl WiringLayer for DevConversionRateApiLayer {
    fn layer_name(&self) -> &'static str {
        "dev_conversion_rate_api_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        context.add_task(Box::new(DevConversionRateApiTask { config: self.0 }));
        Ok(())
    }
}

#[derive(Debug)]
pub struct DevConversionRateApiTask {
    config: BaseTokenFetcherConfig,
}

#[async_trait::async_trait]
impl Task for DevConversionRateApiTask {
    fn name(&self) -> &'static str {
        "dev_conversion_rate_api"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        zksync_dev_conversion_rate_api::start_server(stop_receiver.0, &self.config).await
    }
}
