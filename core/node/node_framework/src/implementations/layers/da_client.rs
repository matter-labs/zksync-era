use zksync_config::{
    configs::{
        chain::StateKeeperConfig,
        da_dispatcher::{DADispatcherConfig, DataAvailabilityMode},
        eth_sender::PubdataSendingMode,
    },
    EthConfig,
};
use zksync_da_client::{gcs::GCSDAClient, no_da::NoDAClient};
use zksync_da_layers::{
    clients::celestia::CelestiaClient, config::DALayerConfig, DataAvailabilityClient,
};

use crate::{
    implementations::resources::da_client::DAClientResource,
    service::ServiceContext,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct DataAvailabilityClientLayer {
    da_config: DADispatcherConfig,
    eth_config: EthConfig,
    state_keeper_config: StateKeeperConfig,
}

impl DataAvailabilityClientLayer {
    pub fn new(
        da_config: DADispatcherConfig,
        eth_config: EthConfig,
        state_keeper_config: StateKeeperConfig,
    ) -> Self {
        Self {
            da_config,
            eth_config,
            state_keeper_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for DataAvailabilityClientLayer {
    fn layer_name(&self) -> &'static str {
        "da_client_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        if self
            .eth_config
            .sender
            .ok_or(WiringError::Configuration(
                "missing the eth_sender config".to_string(),
            ))?
            .pubdata_sending_mode
            != PubdataSendingMode::Custom
        {
            panic!("DA client layer requires custom pubdata sending mode");
        }

        // this can be broken down into the separate layers, but that would require the operator to
        // wire the right one manually, which is less convenient than the current approach, which
        // uses the config to determine the right client
        let client: Box<dyn DataAvailabilityClient> = match self.da_config.da_mode {
            DataAvailabilityMode::GCS(config) => Box::new(GCSDAClient::new(config).await),
            DataAvailabilityMode::NoDA => Box::new(NoDAClient::new()),
            DataAvailabilityMode::DALayer(config) => match config {
                DALayerConfig::Celestia(celestia_config) => {
                    Box::new(CelestiaClient::new(celestia_config))
                }
            },
        };

        if self.state_keeper_config.max_pubdata_per_batch > client.blob_size_limit() as u64 {
            panic!("State keeper max pubdata per batch is greater than the client blob size limit");
        }

        context.insert_resource(DAClientResource(client))?;

        Ok(())
    }
}
