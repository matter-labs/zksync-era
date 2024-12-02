use anyhow::Context;
use zksync_config::configs::{
    self,
    da_client::{
        avail::{AvailClientConfig, AvailConfig, AvailDefaultConfig, AvailGasRelayConfig},
        celestia::CelestiaConfig,
        eigen::EigenConfig,
        DAClientConfig::{Avail, Celestia, Eigen, ObjectStore},
    },
};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::{
    da_client::{self as proto, Link, Path},
    object_store as object_store_proto,
};

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
                disperser_rpc: required(&conf.disperser_rpc)
                    .context("disperser_rpc")?
                    .clone(),
                settlement_layer_confirmation_depth: *required(
                    &conf.settlement_layer_confirmation_depth,
                )
                .context("settlement_layer_confirmation_depth")?,
                eigenda_eth_rpc: required(&conf.eigenda_eth_rpc)
                    .context("eigenda_eth_rpc")?
                    .clone(),
                eigenda_svc_manager_address: required(&conf.eigenda_svc_manager_address)
                    .context("eigenda_svc_manager_address")?
                    .clone(),
                status_query_timeout: *required(&conf.status_query_timeout)
                    .context("status_query_timeout")?,
                status_query_interval: *required(&conf.status_query_interval)
                    .context("status_query_interval")?,
                wait_for_finalization: *required(&conf.wait_for_finalization)
                    .context("wait_for_finalization")?,
                authenticated: *required(&conf.authenticated).context("authenticated")?,
                verify_cert: *required(&conf.verify_cert).context("verify_cert")?,
                points_source: match conf.points_source.clone() {
                    Some(proto::eigen_config::PointsSource::Path(path)) => {
                        let path = required(&path.path).context("path")?;
                        zksync_config::configs::da_client::eigen::PointsSource::Path(path.clone())
                    }
                    Some(proto::eigen_config::PointsSource::Link(link)) => {
                        let link = required(&link.link).context("link")?;
                        zksync_config::configs::da_client::eigen::PointsSource::Link(link.clone())
                    }
                    None => return Err(anyhow::anyhow!("Invalid Eigen DA configuration")),
                },
                chain_id: *required(&conf.chain_id).context("chain_id")?,
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
                disperser_rpc: Some(config.disperser_rpc.clone()),
                settlement_layer_confirmation_depth: Some(
                    config.settlement_layer_confirmation_depth,
                ),
                eigenda_eth_rpc: Some(config.eigenda_eth_rpc.clone()),
                eigenda_svc_manager_address: Some(config.eigenda_svc_manager_address.clone()),
                status_query_timeout: Some(config.status_query_timeout),
                status_query_interval: Some(config.status_query_interval),
                wait_for_finalization: Some(config.wait_for_finalization),
                authenticated: Some(config.authenticated),
                verify_cert: Some(config.verify_cert),
                points_source: Some(match &config.points_source {
                    zksync_config::configs::da_client::eigen::PointsSource::Path(path) => {
                        proto::eigen_config::PointsSource::Path(Path {
                            path: Some(path.to_string()),
                        })
                    }
                    zksync_config::configs::da_client::eigen::PointsSource::Link(link) => {
                        proto::eigen_config::PointsSource::Link(Link {
                            link: Some(link.to_string()),
                        })
                    }
                }),
                chain_id: Some(config.chain_id),
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
