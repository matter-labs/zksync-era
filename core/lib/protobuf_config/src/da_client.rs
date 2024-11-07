use anyhow::Context;
use zksync_config::configs::{
    self,
    da_client::{
        avail::{AvailClientConfig, AvailConfig, AvailDefaultConfig, AvailGasRelayConfig},
        celestia::CelestiaConfig,
        eigen::{DisperserConfig, EigenConfig, MemStoreConfig},
        DAClientConfig::{Avail, Celestia, Eigen, ObjectStore},
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
            proto::data_availability_client::Config::Eigen(conf) => {
                let config = required(&conf.config).context("config")?;
                let eigen_config = match config {
                    proto::eigen_config::Config::MemStore(conf) => {
                        EigenConfig::MemStore(MemStoreConfig {
                            max_blob_size_bytes: required(&conf.max_blob_size_bytes)
                                .context("max_blob_size_bytes")?
                                .clone(),
                            blob_expiration: required(&conf.blob_expiration)
                                .context("blob_expiration")?
                                .clone(),
                            get_latency: required(&conf.get_latency)
                                .context("get_latency")?
                                .clone(),
                            put_latency: required(&conf.put_latency)
                                .context("put_latency")?
                                .clone(),
                        })
                    }
                    proto::eigen_config::Config::Disperser(conf) => {
                        EigenConfig::Disperser(DisperserConfig {
                            disperser_rpc: required(&conf.disperser_rpc)
                                .context("disperser_rpc")?
                                .clone(),
                            eth_confirmation_depth: required(&conf.eth_confirmation_depth)
                                .context("eth_confirmation_depth")?
                                .clone(),
                            eigenda_eth_rpc: required(&conf.eigenda_eth_rpc)
                                .context("eigenda_eth_rpc")?
                                .clone(),
                            eigenda_svc_manager_address: required(
                                &conf.eigenda_svc_manager_address,
                            )
                            .context("eigenda_svc_manager_address")?
                            .clone(),
                            blob_size_limit: required(&conf.blob_size_limit)
                                .context("blob_size_limit")?
                                .clone(),
                            status_query_timeout: required(&conf.status_query_timeout)
                                .context("status_query_timeout")?
                                .clone(),
                            status_query_interval: required(&conf.status_query_interval)
                                .context("status_query_interval")?
                                .clone(),
                            wait_for_finalization: required(&conf.wait_for_finalization)
                                .context("wait_for_finalization")?
                                .clone(),
                            authenticated: required(&conf.authenticated)
                                .context("authenticated")?
                                .clone(),
                            verify_cert: required(&conf.verify_cert)
                                .context("verify_cert")?
                                .clone(),
                            path_to_points: required(&conf.path_to_points)
                                .context("path_to_points")?
                                .clone(),
                        })
                    }
                };
                Eigen(eigen_config)
            }
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
            Eigen(config) => match config {
                EigenConfig::MemStore(config) => {
                    proto::data_availability_client::Config::Eigen(proto::EigenConfig {
                        config: Some(proto::eigen_config::Config::MemStore(
                            proto::MemStoreConfig {
                                max_blob_size_bytes: Some(config.max_blob_size_bytes),
                                blob_expiration: Some(config.blob_expiration),
                                get_latency: Some(config.get_latency),
                                put_latency: Some(config.put_latency),
                            },
                        )),
                    })
                }
                EigenConfig::Disperser(config) => {
                    proto::data_availability_client::Config::Eigen(proto::EigenConfig {
                        config: Some(proto::eigen_config::Config::Disperser(
                            proto::DisperserConfig {
                                disperser_rpc: Some(config.disperser_rpc.clone()),
                                eth_confirmation_depth: Some(config.eth_confirmation_depth),
                                eigenda_eth_rpc: Some(config.eigenda_eth_rpc.clone()),
                                eigenda_svc_manager_address: Some(
                                    config.eigenda_svc_manager_address.clone(),
                                ),
                                blob_size_limit: Some(config.blob_size_limit),
                                status_query_timeout: Some(config.status_query_timeout),
                                status_query_interval: Some(config.status_query_interval),
                                wait_for_finalization: Some(config.wait_for_finalization),
                                authenticated: Some(config.authenticated),
                                verify_cert: Some(config.verify_cert),
                                path_to_points: Some(config.path_to_points.clone()),
                            },
                        )),
                    })
                }
            },
            ObjectStore(config) => proto::data_availability_client::Config::ObjectStore(
                object_store_proto::ObjectStore::build(config),
            ),
        };

        Self {
            config: Some(config),
        }
    }
}
