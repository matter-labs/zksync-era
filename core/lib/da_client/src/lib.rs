use zksync_config::configs::da_dispatcher::{DADispatcherConfig, DataAvailabilityMode};
use zksync_da_layers::DataAvailabilityInterface;

mod clients;

pub async fn new_da_client(config: DADispatcherConfig) -> Box<dyn DataAvailabilityInterface> {
    match config.da_mode {
        DataAvailabilityMode::GCS(config) => Box::new(clients::gcs::GCSDAClient::new(config).await),
        DataAvailabilityMode::NoDA => Box::new(clients::no_da::NoDAClient::new()),
        DataAvailabilityMode::DALayer(config) => {
            zksync_da_layers::new_da_layer_client(config.name, config.private_key.into_bytes())
                .await
        }
    }
}
