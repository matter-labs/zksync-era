use anyhow::Context;
use zksync_config::{
    configs::{
        da_client::DAClient::{Avail, NoDA, ObjectStore},
        {self},
    },
    AvailConfig,
};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::{da_client as proto, object_store as object_store_proto};

impl ProtoRepr for proto::DataAvailabilityClient {
    type Type = configs::DAClientConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let config = if let Some(config) = self.config.clone() {
            match config {
                proto::data_availability_client::Config::Avail(conf) => Avail(AvailConfig {
                    api_node_url: required(&conf.api_node_url)
                        .context("api_node_url")?
                        .clone(),
                    bridge_api_url: required(&conf.bridge_api_url)
                        .context("bridge_api_url")?
                        .clone(),
                    seed: required(&conf.seed).context("seed")?.clone(),
                    app_id: *required(&conf.app_id).context("app_id")?,
                    timeout: *required(&conf.timeout).context("timeout")? as usize,
                    max_retries: *required(&conf.max_retries).context("max_retries")? as usize,
                }),
                proto::data_availability_client::Config::ObjectStore(conf) => {
                    ObjectStore(object_store_proto::ObjectStore::read(&conf)?)
                }
            }
        } else {
            NoDA
        };

        return Ok(configs::DAClientConfig { client: config });
    }

    fn build(this: &Self::Type) -> Self {
        match &this.client {
            NoDA => Self { config: None },
            Avail(config) => Self {
                config: Some(proto::data_availability_client::Config::Avail(
                    proto::AvailConfig {
                        api_node_url: Some(config.api_node_url.clone()),
                        bridge_api_url: Some(config.bridge_api_url.clone()),
                        seed: Some(config.seed.clone()),
                        app_id: Some(config.app_id),
                        timeout: Some(config.timeout as u64),
                        max_retries: Some(config.max_retries as u64),
                    },
                )),
            },
            ObjectStore(config) => Self {
                config: Some(proto::data_availability_client::Config::ObjectStore(
                    object_store_proto::ObjectStore::build(&config),
                )),
            },
        }
    }
}
