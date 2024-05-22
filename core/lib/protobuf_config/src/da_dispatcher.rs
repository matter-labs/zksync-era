use anyhow::Context;
use zksync_config::configs;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::{da_dispatcher as proto, object_store::ObjectStore};

impl ProtoRepr for proto::DataAvailabilityDispatcher {
    type Type = configs::da_dispatcher::DADispatcherConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        match &self.credentials {
            Some(proto::data_availability_dispatcher::Credentials::DaLayer(config)) => {
                Ok(configs::da_dispatcher::DADispatcherConfig {
                    da_mode: configs::da_dispatcher::DataAvailabilityMode::DALayer(
                        configs::da_dispatcher::DALayerInfo {
                            name: required(&config.name).context("name")?.clone(),
                            private_key: required(&config.private_key)
                                .context("private_key")?
                                .clone(),
                        },
                    ),
                    polling_interval: Some(
                        *required(&self.polling_interval).context("polling_interval")?,
                    ),
                    query_rows_limit: Some(
                        *required(&self.query_rows_limit).context("query_rows_limit")?,
                    ),
                    max_retries: Some(
                        *required(&self.max_retries).context("query_rows_limit")? as u16
                    ),
                })
            }
            Some(proto::data_availability_dispatcher::Credentials::ObjectStore(config)) => {
                Ok(configs::da_dispatcher::DADispatcherConfig {
                    da_mode: configs::da_dispatcher::DataAvailabilityMode::GCS(config.read()?),
                    polling_interval: Some(
                        *required(&self.polling_interval).context("polling_interval")?,
                    ),
                    query_rows_limit: Some(
                        *required(&self.query_rows_limit).context("query_rows_limit")?,
                    ),
                    max_retries: Some(
                        *required(&self.max_retries).context("query_rows_limit")? as u16
                    ),
                })
            }
            None => Ok(configs::da_dispatcher::DADispatcherConfig {
                da_mode: configs::da_dispatcher::DataAvailabilityMode::NoDA,
                polling_interval: None,
                query_rows_limit: None,
                max_retries: None,
            }),
        }
    }

    fn build(this: &Self::Type) -> Self {
        let credentials = match this.da_mode.clone() {
            configs::da_dispatcher::DataAvailabilityMode::DALayer(info) => Some(
                proto::data_availability_dispatcher::Credentials::DaLayer(proto::DaLayer {
                    name: Some(info.name.clone()),
                    private_key: Some(info.private_key.clone()),
                }),
            ),
            configs::da_dispatcher::DataAvailabilityMode::GCS(config) => Some(
                proto::data_availability_dispatcher::Credentials::ObjectStore(ObjectStore::build(
                    &config,
                )),
            ),
            configs::da_dispatcher::DataAvailabilityMode::NoDA => None,
        };

        Self {
            credentials,
            polling_interval: this.polling_interval,
            query_rows_limit: this.query_rows_limit,
            max_retries: this.max_retries.map(|x| x as u32),
        }
    }
}
