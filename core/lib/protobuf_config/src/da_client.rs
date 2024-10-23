use anyhow::Context;
use zksync_config::{
    configs::{
        da_client::DAClientConfig::{Avail, Celestia, ObjectStore},
        {self},
    },
    AvailConfig, CelestiaConfig,
    DAClientConfig::Eigen,
    EigenConfig,
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
                app_id: *required(&conf.app_id).context("app_id")?,
                timeout: *required(&conf.timeout).context("timeout")? as usize,
                max_retries: *required(&conf.max_retries).context("max_retries")? as usize,
            }),
            proto::data_availability_client::Config::Celestia(conf) => Celestia(CelestiaConfig {
                api_node_url: required(&conf.api_node_url).context("namespace")?.clone(),
                namespace: required(&conf.namespace).context("namespace")?.clone(),
                chain_id: required(&conf.chain_id).context("chain_id")?.clone(),
                timeout_ms: *required(&conf.timeout_ms).context("timeout_ms")?,
            }),
            proto::data_availability_client::Config::Eigen(conf) => Eigen(EigenConfig {
                rpc_node_url: required(&conf.rpc_node_url)
                    .context("rpc_node_url")?
                    .clone(),
                inclusion_polling_interval_ms: *required(&conf.inclusion_polling_interval_ms)
                    .context("inclusion_polling_interval_ms")?,
            }),
            proto::data_availability_client::Config::ObjectStore(conf) => {
                ObjectStore(object_store_proto::ObjectStore::read(conf)?)
            }
        };

        Ok(client)
    }

    fn build(this: &Self::Type) -> Self {
        let config = match &this {
            Avail(config) => proto::data_availability_client::Config::Avail(proto::AvailConfig {
                api_node_url: Some(config.api_node_url.clone()),
                bridge_api_url: Some(config.bridge_api_url.clone()),
                app_id: Some(config.app_id),
                timeout: Some(config.timeout as u64),
                max_retries: Some(config.max_retries as u64),
            }),
            Celestia(config) => {
                proto::data_availability_client::Config::Celestia(proto::CelestiaConfig {
                    api_node_url: Some(config.api_node_url.clone()),
                    namespace: Some(config.namespace.clone()),
                    chain_id: Some(config.chain_id.clone()),
                    timeout_ms: Some(config.timeout_ms),
                })
            }
            Eigen(config) => proto::data_availability_client::Config::Eigen(proto::EigenConfig {
                rpc_node_url: Some(config.rpc_node_url.clone()),
                inclusion_polling_interval_ms: Some(config.inclusion_polling_interval_ms),
            }),
            ObjectStore(config) => proto::data_availability_client::Config::ObjectStore(
                object_store_proto::ObjectStore::build(config),
            ),
        };

        Self {
            config: Some(config),
        }
    }
}
