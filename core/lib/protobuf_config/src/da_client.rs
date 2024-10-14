use anyhow::Context;
use zksync_config::configs::{
    self,
    da_client::{
        avail::{AvailClientConfig, AvailConfig, AvailDefaultConfig, AvailGasRelayConfig},
        DAClientConfig::{Avail, ObjectStore},
    },
};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::{da_client as proto, object_store as object_store_proto};

impl ProtoRepr for proto::DataAvailabilityClient {
    type Type = configs::DAClientConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let config = required(&self.config).context("config")?;

        let client = match config {
            proto::data_availability_client::Config::Avail(conf) => {
                Avail(if conf.gas_relay_mode.unwrap_or(false) {
                    AvailConfig {
                        bridge_api_url: required(&conf.bridge_api_url)
                            .context("bridge_api_url")?
                            .clone(),
                        gas_relay_mode: true,
                        config: AvailClientConfig::GasRelay(AvailGasRelayConfig {
                            gas_relay_api_url: required(&conf.gas_relay_api_url)
                                .context("gas_relay_api_url")?
                                .clone(),
                        }),
                    }
                } else {
                    AvailConfig {
                        bridge_api_url: required(&conf.bridge_api_url)
                            .context("bridge_api_url")?
                            .clone(),
                        gas_relay_mode: false,
                        config: AvailClientConfig::Default(AvailDefaultConfig {
                            api_node_url: required(&conf.api_node_url)
                                .context("api_node_url")?
                                .clone(),
                            app_id: *required(&conf.app_id).context("app_id")?,
                            timeout: *required(&conf.timeout).context("timeout")? as usize,
                            max_retries: *required(&conf.max_retries).context("max_retries")?
                                as usize,
                        }),
                    }
                })
            }
            proto::data_availability_client::Config::ObjectStore(conf) => {
                ObjectStore(object_store_proto::ObjectStore::read(conf)?)
            }
        };

        Ok(client)
    }

    fn build(this: &Self::Type) -> Self {
        match &this {
            Avail(config) => Self {
                config: Some(proto::data_availability_client::Config::Avail(
                    proto::AvailConfig {
                        bridge_api_url: Some(config.bridge_api_url.clone()),
                        gas_relay_mode: Some(config.gas_relay_mode),
                        api_node_url: match &config.config {
                            AvailClientConfig::Default(conf) => Some(conf.api_node_url.clone()),
                            AvailClientConfig::GasRelay(_) => None,
                        },
                        app_id: match &config.config {
                            AvailClientConfig::Default(conf) => Some(conf.app_id),
                            AvailClientConfig::GasRelay(_) => None,
                        },
                        timeout: match &config.config {
                            AvailClientConfig::Default(conf) => Some(conf.timeout as u64),
                            AvailClientConfig::GasRelay(_) => None,
                        },
                        max_retries: match &config.config {
                            AvailClientConfig::Default(conf) => Some(conf.max_retries as u64),
                            AvailClientConfig::GasRelay(_) => None,
                        },
                        gas_relay_api_url: match &config.config {
                            AvailClientConfig::GasRelay(conf) => {
                                Some(conf.gas_relay_api_url.clone())
                            }
                            AvailClientConfig::Default(_) => None,
                        },
                    },
                )),
            },
            ObjectStore(config) => Self {
                config: Some(proto::data_availability_client::Config::ObjectStore(
                    object_store_proto::ObjectStore::build(config),
                )),
            },
        }
    }
}
