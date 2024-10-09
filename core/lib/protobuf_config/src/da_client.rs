use anyhow::Context;
use zksync_config::configs::{
    da_client::avail::AvailConfig,
    da_client::DAClientConfig::{Avail, ObjectStore},
    {self},
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
                        api_node_url: None,
                        bridge_api_url: required(&conf.bridge_api_url)
                            .context("bridge_api_url")?
                            .clone(),
                        app_id: None,
                        timeout: *required(&conf.timeout).context("timeout")? as usize,
                        max_retries: *required(&conf.max_retries).context("max_retries")? as usize,
                        gas_relay_mode: conf
                            .gas_relay_mode
                            .context("gas_relay_mode")
                            .unwrap_or(false),
                        // if gas_relay_mode is true, then we need to set the gas_relay_api_url and gas_relay_api_key
                        gas_relay_api_url: Some(
                            required(&conf.gas_relay_api_url)
                                .context("gas_relay_api_url")?
                                .clone(),
                        ),
                        gas_relay_api_key: Some(
                            required(&conf.gas_relay_api_key)
                                .context("gas_relay_api_key")?
                                .clone(),
                        ),
                    }
                } else {
                    AvailConfig {
                        api_node_url: Some(
                            required(&conf.api_node_url)
                                .context("api_node_url")?
                                .clone(),
                        ),
                        bridge_api_url: required(&conf.bridge_api_url)
                            .context("bridge_api_url")?
                            .clone(),
                        app_id: Some(*required(&conf.app_id).context("app_id")?),
                        timeout: *required(&conf.timeout).context("timeout")? as usize,
                        max_retries: *required(&conf.max_retries).context("max_retries")? as usize,
                        gas_relay_mode: conf
                            .gas_relay_mode
                            .context("gas_relay_mode")
                            .unwrap_or(false),
                        // if gas_relay_mode is not true, then the gas_relay_api_url and gas_relay_api_key are not required
                        gas_relay_api_url: None,
                        gas_relay_api_key: None,
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
                        api_node_url: config.api_node_url.clone(),
                        bridge_api_url: Some(config.bridge_api_url.clone()),
                        app_id: config.app_id,
                        timeout: Some(config.timeout as u64),
                        max_retries: Some(config.max_retries as u64),
                        gas_relay_mode: Some(config.gas_relay_mode),
                        gas_relay_api_url: config.gas_relay_api_url.clone(),
                        gas_relay_api_key: config.gas_relay_api_key.clone(),
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
