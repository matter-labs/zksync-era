use secrecy::ExposeSecret;
use zksync_config::configs;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::da_client as proto;

impl ProtoRepr for proto::AvailSecrets {
    type Type = configs::da_client::avail::AvailSecrets;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(configs::da_client::avail::AvailSecrets {
            seed_phrase: required(&self.seed_phrase).expect("seed_phrase").parse()?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            seed_phrase: Some(this.seed_phrase.0.expose_secret().to_string()),
        }
    }
}

impl ProtoRepr for proto::CelestiaSecrets {
    type Type = configs::da_client::celestia::CelestiaSecrets;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(configs::da_client::celestia::CelestiaSecrets {
            private_key: required(&self.private_key).expect("private_key").parse()?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            private_key: Some(this.private_key.0.expose_secret().to_string()),
        }
    }
}
