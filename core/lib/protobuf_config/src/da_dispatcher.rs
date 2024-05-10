use anyhow::Context;
use zksync_config::configs;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::{da_dispatcher as proto, object_store::ObjectStore};

impl ProtoRepr for proto::DataAvailabilityDispatcher {
    type Type = configs::da_dispatcher::DADispatcherConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        configs::da_dispatcher::DADispatcherConfig {
            credentials: match &self.credentials {
                Some(proto::data_availability_dispatcher::Credentials::DaLayer(config)) => {
                    configs::da_dispatcher::DACredentials::DALayer(
                        configs::da_dispatcher::DALayerInfo {
                            url: *required(&config.url).context("url"),
                            private_key: required(&config.private_key)
                                .context("private_key")
                                .into_bytes(),
                        },
                    )
                }
                Some(proto::data_availability_dispatcher::Credentials::ObjectStore(config)) => {
                    configs::da_dispatcher::DACredentials::GCS(config.read()?)
                }
                None => None,
            },
        }
    }

    fn build(this: &Self::Type) -> Self {
        let credentials = match this.credentials.clone() {
            configs::da_dispatcher::DACredentials::DALayer(info) => Some(
                proto::data_availability_dispatcher::Credentials::DaLayer(proto::DaLayer {
                    url: Some(info.url.clone()),
                    private_key: info.private_key.clone().into(),
                }),
            ),
            configs::da_dispatcher::DACredentials::GCS(config) => Some(
                proto::data_availability_dispatcher::Credentials::ObjectStore(ObjectStore::build(
                    &config,
                )),
            ),
        };

        Self { credentials }
    }
}
