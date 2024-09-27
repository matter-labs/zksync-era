use std::str::FromStr;

use anyhow::Context;
use secrecy::ExposeSecret;
use zksync_basic_types::{
    secrets::{PrivateKey, SeedPhrase},
    url::SensitiveUrl,
};
use zksync_config::configs::{
    consensus::{AttesterSecretKey, ConsensusSecrets, NodeSecretKey, ValidatorSecretKey},
    da_client::{avail::AvailSecrets, celestia::CelestiaSecrets},
    secrets::{DataAvailabilitySecrets, Secrets},
    DatabaseSecrets, L1Secrets,
};
use zksync_protobuf::{required, ProtoRepr};

use crate::{
    proto::{secrets as proto, secrets::data_availability_secrets::DaSecrets},
    read_optional_repr,
};

impl ProtoRepr for proto::Secrets {
    type Type = Secrets;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            consensus: read_optional_repr(&self.consensus),
            database: read_optional_repr(&self.database),
            l1: read_optional_repr(&self.l1),
            data_availability: read_optional_repr(&self.data_availability),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            database: this.database.as_ref().map(ProtoRepr::build),
            l1: this.l1.as_ref().map(ProtoRepr::build),
            consensus: this.consensus.as_ref().map(ProtoRepr::build),
            data_availability: this.data_availability.as_ref().map(ProtoRepr::build),
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
            DaSecrets::Avail(avail) => DataAvailabilitySecrets::Avail(AvailSecrets {
                seed_phrase: SeedPhrase::from_str(
                    required(&avail.seed_phrase).context("seed_phrase")?,
                )
                .unwrap(),
            }),
            DaSecrets::Celestia(celestia) => DataAvailabilitySecrets::Celestia(CelestiaSecrets {
                private_key: PrivateKey::from_str(
                    required(&celestia.private_key).context("private_key")?,
                )
                .unwrap(),
            }),
        };

        Ok(client)
    }

    fn build(this: &Self::Type) -> Self {
        let secrets = match &this {
            DataAvailabilitySecrets::Avail(config) => Some(DaSecrets::Avail(proto::AvailSecrets {
                seed_phrase: Some(config.seed_phrase.0.expose_secret().to_string()),
            })),
            DataAvailabilitySecrets::Celestia(config) => {
                Some(DaSecrets::Celestia(proto::CelestiaSecrets {
                    private_key: Some(config.private_key.0.expose_secret().to_string()),
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
