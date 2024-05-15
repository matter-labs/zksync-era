use secrecy::ExposeSecret;
use zksync_config::configs::{
    consensus::{ConsensusSecrets, NodeSecretKey, ValidatorSecretKey},
    secrets::Secrets,
};
use zksync_protobuf::ProtoRepr;

use crate::proto::secrets as proto;

impl ProtoRepr for proto::Secrets {
    type Type = Secrets;

    fn read(&self) -> anyhow::Result<Self::Type> {
        todo!()
    }

    fn build(this: &Self::Type) -> Self {
        todo!()
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
            node_key: this.node_key.as_ref().map(|x| x.0.expose_secret().clone()),
        }
    }
}
