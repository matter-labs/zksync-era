use anyhow::Context;
use zksync_config::{
    configs::{
        da_client::{
            eigen_da::EigenDAConfig,
            DAClient::{Avail, EigenDA, ObjectStore},
        },
        {self},
    },
    AvailConfig,
};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::{da_client as proto, object_store as object_store_proto};

impl ProtoRepr for proto::DataAvailabilityClient {
    type Type = configs::DAClientConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let config = required(&self.config).context("config")?;

        let client = match config {
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
                ObjectStore(object_store_proto::ObjectStore::read(conf)?)
            }
            proto::data_availability_client::Config::EigenDa(conf) => EigenDA(EigenDAConfig {
                api_node_url: required(&conf.api_node_url)
                    .context("api_node_url")?
                    .clone(),
                custom_quorum_numbers: Some(conf.custom_quorum_numbers.clone()),
                account_id: conf.account_id.clone(),
            }),
        };

        Ok(configs::DAClientConfig { client })
    }

    fn build(this: &Self::Type) -> Self {
        match &this.client {
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
                    object_store_proto::ObjectStore::build(config),
                )),
            },
            EigenDA(config) => Self {
                config: Some(proto::data_availability_client::Config::EigenDa(
                    proto::EigenDaConfig {
                        api_node_url: Some(config.api_node_url.clone()),
                        custom_quorum_numbers: config
                            .custom_quorum_numbers
                            .clone()
                            .unwrap_or_default(),
                        account_id: config.account_id.clone(),
                    },
                )),
            },
        }
    }
}
