use anyhow::Context;
use zksync_config::{
    configs::{
        da_client::DAClientConfig::{Avail, Near, ObjectStore},
        {self},
    },
    AvailConfig, NearConfig,
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
                secret_key: required(&conf.secret_key).context("secret_key")?.clone(),
            }),
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
            Near(config) => Self {
                config: Some(proto::data_availability_client::Config::Near(
                    proto::NearConfig {
                        evm_provider_url: Some(config.evm_provider_url.clone()),
                        rpc_client_url: Some(config.rpc_client_url.clone()),
                        blob_contract: Some(config.blob_contract.clone()),
                        bridge_contract: Some(config.bridge_contract.clone()),
                        account_id: Some(config.account_id.clone()),
                        secret_key: Some(config.secret_key.clone()),
                    },
                )),
            },
        }
    }
}
