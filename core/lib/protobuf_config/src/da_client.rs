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
                Avail(match conf.config.as_ref() {
                    Some(proto::avail_config::Config::FullClient(full_client__conf)) => {
                        AvailConfig {
                            bridge_api_url: required(&conf.bridge_api_url)
                                .context("bridge_api_url")?
                                .clone(),
                            timeout: *required(&conf.timeout).context("timeout")? as usize,
                            config: AvailClientConfig::FullClient(AvailDefaultConfig {
                                api_node_url: required(&full_client__conf.api_node_url)
                                    .context("api_node_url")?
                                    .clone(),
                                app_id: *required(&full_client__conf.app_id).context("app_id")?,
                            }),
                        }
                    }
                    Some(proto::avail_config::Config::GasRelay(gas_relay_conf)) => AvailConfig {
                        bridge_api_url: required(&conf.bridge_api_url)
                            .context("bridge_api_url")?
                            .clone(),
                        timeout: *required(&conf.timeout).context("timeout")? as usize,
                        config: AvailClientConfig::GasRelay(AvailGasRelayConfig {
                            gas_relay_api_url: required(&gas_relay_conf.gas_relay_api_url)
                                .context("gas_relay_api_url")?
                                .clone(),
                        }),
                    },
                    None => return Err(anyhow::anyhow!("Invalid Avail DA configuration")),
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
                        timeout: Some(config.timeout as u64),
                        config: match &config.config {
                            AvailClientConfig::FullClient(conf) => Some(
                                proto::avail_config::Config::FullClient(proto::AvailClientConfig {
                                    api_node_url: Some(conf.api_node_url.clone()),
                                    app_id: Some(conf.app_id),
                                }),
                            ),
                            AvailClientConfig::GasRelay(conf) => Some(
                                proto::avail_config::Config::GasRelay(proto::AvailGasRelayConfig {
                                    gas_relay_api_url: Some(conf.gas_relay_api_url.clone()),
                                }),
                            ),
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
