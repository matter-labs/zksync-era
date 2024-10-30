use anyhow::Context;
use zksync_config::{
    configs::{
        self,
        da_client::{
            eigen_da::{DisperserConfig, EigenDAConfig, MemStoreConfig},
            DAClientConfig::{Avail, EigenDA, ObjectStore},
        },
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
                app_id: *required(&conf.app_id).context("app_id")?,
                timeout: *required(&conf.timeout).context("timeout")? as usize,
                max_retries: *required(&conf.max_retries).context("max_retries")? as usize,
            }),
            proto::data_availability_client::Config::ObjectStore(conf) => {
                ObjectStore(object_store_proto::ObjectStore::read(conf)?)
            }
            proto::data_availability_client::Config::EigenDa(conf) => {
                let config = required(&conf.config).context("config")?;
                let eigenda_config = match config {
                    proto::eigen_da_config::Config::MemStore(conf) => {
                        EigenDAConfig::MemStore(MemStoreConfig {
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
                    proto::eigen_da_config::Config::Disperser(conf) => {
                        EigenDAConfig::Disperser(DisperserConfig {
                            custom_quorum_numbers: Some(conf.custom_quorum_numbers.clone()),
                            account_id: conf.account_id.clone(),
                            disperser_rpc: required(&conf.disperser_rpc)
                                .context("disperser_rpc")?
                                .clone(),
                            eth_confirmation_depth: required(&conf.eth_confirmation_depth)
                                .context("eth_confirmation_depth")?
                                .clone(),
                            eigenda_eth_rpc: required(&conf.eigenda_eth_rpc)
                                .context("eigenda_eth_rpc")?
                                .clone(),
                            eigenda_svc_manager_addr: required(&conf.eigenda_svc_manager_addr)
                                .context("eigenda_svc_manager_addr")?
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
                            authenticaded: required(&conf.authenticated)
                                .context("authenticaded")?
                                .clone(),
                        })
                    }
                };
                EigenDA(eigenda_config)
            }
        };

        Ok(client)
    }

    fn build(this: &Self::Type) -> Self {
        match &this {
            Avail(config) => Self {
                config: Some(proto::data_availability_client::Config::Avail(
                    proto::AvailConfig {
                        api_node_url: Some(config.api_node_url.clone()),
                        bridge_api_url: Some(config.bridge_api_url.clone()),
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
            EigenDA(config) => match config {
                EigenDAConfig::MemStore(config) => Self {
                    config: Some(proto::data_availability_client::Config::EigenDa(
                        proto::EigenDaConfig {
                            config: Some(proto::eigen_da_config::Config::MemStore(
                                proto::MemStoreConfig {
                                    max_blob_size_bytes: Some(config.max_blob_size_bytes),
                                    blob_expiration: Some(config.blob_expiration),
                                    get_latency: Some(config.get_latency),
                                    put_latency: Some(config.put_latency),
                                },
                            )),
                        },
                    )),
                },
                EigenDAConfig::Disperser(config) => Self {
                    config: Some(proto::data_availability_client::Config::EigenDa(
                        proto::EigenDaConfig {
                            config: Some(proto::eigen_da_config::Config::Disperser(
                                proto::DisperserConfig {
                                    custom_quorum_numbers: config
                                        .custom_quorum_numbers
                                        .clone()
                                        .unwrap_or_default(),
                                    account_id: config.account_id.clone(),
                                    disperser_rpc: Some(config.disperser_rpc.clone()),
                                    eth_confirmation_depth: Some(config.eth_confirmation_depth),
                                    eigenda_eth_rpc: Some(config.eigenda_eth_rpc.clone()),
                                    eigenda_svc_manager_addr: Some(
                                        config.eigenda_svc_manager_addr.clone(),
                                    ),
                                    blob_size_limit: Some(config.blob_size_limit),
                                    status_query_timeout: Some(config.status_query_timeout),
                                    status_query_interval: Some(config.status_query_interval),
                                    wait_for_finalization: Some(config.wait_for_finalization),
                                    authenticated: Some(config.authenticaded),
                                },
                            )),
                        },
                    )),
                },
            },
        }
    }
}
