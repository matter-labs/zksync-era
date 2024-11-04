use anyhow::Context;
use zksync_config::configs::{
    self,
    da_client::{
        avail::{AvailClientConfig, AvailConfig, AvailDefaultConfig, AvailGasRelayConfig},
        celestia::CelestiaConfig,
        eigen::EigenConfig,
        near::NearConfig,
        DAClientConfig::{Avail, Celestia, Eigen, Near, ObjectStore},
    },
};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::{da_client as proto, object_store as object_store_proto};

impl ProtoRepr for proto::DataAvailabilityClient {
    type Type = configs::DAClientConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let config = required(&self.config).context("config")?;

        let client = match config {
            proto::data_availability_client::Config::Avail(conf) => Avail(AvailConfig {
                bridge_api_url: required(&conf.bridge_api_url)
                    .context("bridge_api_url")?
                    .clone(),
                timeout_ms: *required(&conf.timeout_ms).context("timeout_ms")? as usize,
                config: match conf.config.as_ref() {
                    Some(proto::avail_config::Config::FullClient(full_client_conf)) => {
                        AvailClientConfig::FullClient(AvailDefaultConfig {
                            api_node_url: required(&full_client_conf.api_node_url)
                                .context("api_node_url")?
                                .clone(),
                            app_id: *required(&full_client_conf.app_id).context("app_id")?,
                        })
                    }
                    Some(proto::avail_config::Config::GasRelay(gas_relay_conf)) => {
                        AvailClientConfig::GasRelay(AvailGasRelayConfig {
                            gas_relay_api_url: required(&gas_relay_conf.gas_relay_api_url)
                                .context("gas_relay_api_url")?
                                .clone(),
                            max_retries: *required(&gas_relay_conf.max_retries)
                                .context("max_retries")?
                                as usize,
                        })
                    }
                    None => return Err(anyhow::anyhow!("Invalid Avail DA configuration")),
                },
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
            proto::data_availability_client::Config::Near(conf) => Near(NearConfig {
                evm_provider_url: required(&conf.evm_provider_url)
                    .context("evm_provider_url")?
                    .clone(),
                rpc_client_url: required(&conf.rpc_client_url)
                    .context("rpc_client_url")?
                    .clone(),
                blob_contract: required(&conf.blob_contract)
                    .context("blob_contract")?
                    .clone(),
                bridge_contract: required(&conf.bridge_contract)
                    .context("bridge_contract")?
                    .clone(),
                account_id: required(&conf.account_id).context("account_id")?.clone(),
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
                bridge_api_url: Some(config.bridge_api_url.clone()),
                timeout_ms: Some(config.timeout_ms as u64),
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
                            max_retries: Some(conf.max_retries as u64),
                        }),
                    ),
                },
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
            Near(config) => proto::data_availability_client::Config::Near(proto::NearConfig {
                evm_provider_url: Some(config.evm_provider_url.clone()),
                rpc_client_url: Some(config.rpc_client_url.clone()),
                blob_contract: Some(config.blob_contract.clone()),
                bridge_contract: Some(config.bridge_contract.clone()),
                account_id: Some(config.account_id.clone()),
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
