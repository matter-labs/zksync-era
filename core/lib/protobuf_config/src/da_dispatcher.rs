use anyhow::{Context, Error};
use zksync_config::configs::{self, da_dispatcher::DataAvailabilityMode};
use zksync_da_layers::config::DALayerConfig;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::{da_dispatcher as proto, object_store::ObjectStore};

impl ProtoRepr for proto::DataAvailabilityDispatcher {
    type Type = configs::da_dispatcher::DADispatcherConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        match &self.mode {
            Some(proto::data_availability_dispatcher::Mode::DaLayer(config)) => {
                let da_config = match required(&config.name).context("da_layer_name")?.as_str() {
                    "celestia" => DALayerConfig::Celestia(
                        zksync_da_layers::clients::celestia::config::CelestiaConfig {
                            light_node_url: required(&config.light_node_url)
                                .context("light_node_url")?
                                .clone(),
                            private_key: required(&config.private_key)
                                .context("private_key")?
                                .clone(),
                        },
                    ),
                    _ => {
                        return Err(Error::msg(format!(
                            "Unknown DA layer name: {}",
                            required(&config.name).context("da_layer_name")?
                        )))
                    }
                };
                Ok(configs::da_dispatcher::DADispatcherConfig {
                    da_mode: DataAvailabilityMode::DALayer(da_config),
                    polling_interval_ms: Some(
                        *required(&self.polling_interval).context("polling_interval")?,
                    ),
                    query_rows_limit: Some(
                        *required(&self.query_rows_limit).context("query_rows_limit")?,
                    ),
                    max_retries: Some(
                        *required(&self.max_retries).context("query_rows_limit")? as u16
                    ),
                })
            }
            Some(proto::data_availability_dispatcher::Mode::ObjectStore(config)) => {
                Ok(configs::da_dispatcher::DADispatcherConfig {
                    da_mode: DataAvailabilityMode::ObjectStore(config.read()?),
                    polling_interval_ms: Some(
                        *required(&self.polling_interval).context("polling_interval")?,
                    ),
                    query_rows_limit: Some(
                        *required(&self.query_rows_limit).context("query_rows_limit")?,
                    ),
                    max_retries: Some(
                        *required(&self.max_retries).context("query_rows_limit")? as u16
                    ),
                })
            }
            None => Ok(configs::da_dispatcher::DADispatcherConfig {
                da_mode: DataAvailabilityMode::NoDA,
                polling_interval_ms: None,
                query_rows_limit: None,
                max_retries: None,
            }),
        }
    }

    fn build(this: &Self::Type) -> Self {
        let mode = match this.da_mode.clone() {
            DataAvailabilityMode::DALayer(info) => match info {
                DALayerConfig::Celestia(info) => Some(
                    proto::data_availability_dispatcher::Mode::DaLayer(proto::DaLayer {
                        name: Some("celestia".to_string()),
                        private_key: Some(info.private_key.clone()),
                        light_node_url: Some(info.light_node_url.clone()),
                    }),
                ),
            },
            DataAvailabilityMode::ObjectStore(config) => Some(
                proto::data_availability_dispatcher::Mode::ObjectStore(ObjectStore::build(&config)),
            ),
            DataAvailabilityMode::NoDA => None,
        };

        Self {
            mode,
            polling_interval: this.polling_interval_ms,
            query_rows_limit: this.query_rows_limit,
            max_retries: this.max_retries.map(|x| x as u32),
        }
    }
}
