use std::str::FromStr;

use anyhow::Context;
use secrecy::ExposeSecret;
use zksync_basic_types::{api_key::APIKey, seed_phrase::SeedPhrase, url::SensitiveUrl};
use zksync_config::configs::{
    consensus::{AttesterSecretKey, ConsensusSecrets, NodeSecretKey, ValidatorSecretKey},
    da_client::avail::AvailSecrets,
    secrets::{DataAvailabilitySecrets, Secrets},
    DatabaseSecrets, L1Secrets,
};
use zksync_protobuf::{required, ProtoRepr};

use crate::{
    proto::{
        secrets as proto,
        secrets::{data_availability_secrets::DaSecrets, AvailSecret},
    },
    read_optional_repr,
};

impl ProtoRepr for proto::Secrets {
    type Type = Secrets;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            consensus: read_optional_repr(&self.consensus),
            database: read_optional_repr(&self.database),
            l1: read_optional_repr(&self.l1),
            data_availability: read_optional_repr(&self.da),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            database: this.database.as_ref().map(ProtoRepr::build),
            l1: this.l1.as_ref().map(ProtoRepr::build),
            consensus: this.consensus.as_ref().map(ProtoRepr::build),
            da: this.data_availability.as_ref().map(ProtoRepr::build),
        }
    }
}

impl ProtoRepr for proto::DatabaseSecrets {
    type Type = DatabaseSecrets;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let server_url = self
            .server_url
            .as_deref()
            .map(str::parse::<SensitiveUrl>)
            .transpose()
            .context("master_url")?;
        let server_replica_url = self
            .server_replica_url
            .as_deref()
            .map(str::parse::<SensitiveUrl>)
            .transpose()
            .context("replica_url")?;
        let prover_url = self
            .prover_url
            .as_deref()
            .map(str::parse::<SensitiveUrl>)
            .transpose()
            .context("prover_url")?;
        Ok(Self::Type {
            server_url,
            prover_url,
            server_replica_url,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            server_url: this.server_url.as_ref().map(|a| a.expose_str().to_string()),
            server_replica_url: this
                .server_replica_url
                .as_ref()
                .map(|a| a.expose_str().to_string()),
            prover_url: this.prover_url.as_ref().map(|a| a.expose_str().to_string()),
        }
    }
}

impl ProtoRepr for proto::L1Secrets {
    type Type = L1Secrets;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            l1_rpc_url: SensitiveUrl::from_str(required(&self.l1_rpc_url).context("l1_rpc_url")?)?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            l1_rpc_url: Some(this.l1_rpc_url.expose_str().to_string()),
        }
    }
}

impl ProtoRepr for proto::DataAvailabilitySecrets {
    type Type = DataAvailabilitySecrets;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let secrets = required(&self.da_secrets).context("config")?;

        let client = match secrets {
            DaSecrets::Avail(avail_secret) => {
                let seed_phrase = match avail_secret.seed_phrase.as_ref() {
                    Some(seed) => match SeedPhrase::from_str(seed) {
                        Ok(seed) => Some(seed),
                        Err(_) => None,
                    },
                    None => None,
                };
                let gas_relay_api_key = match avail_secret.gas_relay_api_key.as_ref() {
                    Some(api_key) => match APIKey::from_str(api_key) {
                        Ok(api_key) => Some(api_key),
                        Err(_) => None,
                    },
                    None => None,
                };
                if seed_phrase.is_none() && gas_relay_api_key.is_none() {
                    return Err(anyhow::anyhow!(
                        "At least one of seed_phrase or gas_relay_api_key must be provided"
                    ));
                }
                DataAvailabilitySecrets::Avail(AvailSecrets {
                    seed_phrase,
                    gas_relay_api_key,
                })
            }
        };

        Ok(client)
    }

    fn build(this: &Self::Type) -> Self {
        let secrets = match &this {
            DataAvailabilitySecrets::Avail(config) => {
                let seed_phrase = if config.seed_phrase.is_some() {
                    Some(
                        config
                            .clone()
                            .seed_phrase
                            .unwrap()
                            .0
                            .expose_secret()
                            .to_string(),
                    )
                } else {
                    None
                };

                let gas_relay_api_key = if config.gas_relay_api_key.is_some() {
                    Some(
                        config
                            .clone()
                            .gas_relay_api_key
                            .unwrap()
                            .0
                            .expose_secret()
                            .to_string(),
                    )
                } else {
                    None
                };

                Some(DaSecrets::Avail(AvailSecret {
                    seed_phrase,
                    gas_relay_api_key,
                }))
            }
        };

        Self {
            da_secrets: secrets,
        }
    }
}

impl ProtoRepr for proto::ConsensusSecrets {
    type Type = ConsensusSecrets;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            validator_key: self
                .validator_key
                .as_ref()
                .map(|x| ValidatorSecretKey(x.clone().into())),
            attester_key: self
                .attester_key
                .as_ref()
                .map(|x| AttesterSecretKey(x.clone().into())),
            node_key: self
                .node_key
                .as_ref()
                .map(|x| NodeSecretKey(x.clone().into())),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            validator_key: this
                .validator_key
                .as_ref()
                .map(|x| x.0.expose_secret().clone()),
            attester_key: this
                .attester_key
                .as_ref()
                .map(|x| x.0.expose_secret().clone()),
            node_key: this.node_key.as_ref().map(|x| x.0.expose_secret().clone()),
        }
    }
}
