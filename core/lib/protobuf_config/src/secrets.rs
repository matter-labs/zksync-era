use std::str::FromStr;

use anyhow::Context;
use secrecy::ExposeSecret;
use zksync_basic_types::url::SensitiveUrl;
use zksync_config::configs::{
    consensus::{AttesterSecretKey, ConsensusSecrets, NodeSecretKey, ValidatorSecretKey},
    secrets::Secrets,
    DatabaseSecrets, L1Secrets,
};
use zksync_protobuf::{required, ProtoRepr};

use crate::{proto::secrets as proto, read_optional_repr};

impl ProtoRepr for proto::Secrets {
    type Type = Secrets;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            consensus: read_optional_repr(&self.consensus),
            database: read_optional_repr(&self.database),
            l1: read_optional_repr(&self.l1),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            database: this.database.as_ref().map(ProtoRepr::build),
            l1: this.l1.as_ref().map(ProtoRepr::build),
            consensus: this.consensus.as_ref().map(ProtoRepr::build),
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
            gateway_url: None,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            l1_rpc_url: Some(this.l1_rpc_url.expose_str().to_string()),
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
