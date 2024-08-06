use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::prover as proto;

impl ProtoRepr for proto::TeeVerifierInputProducer {
    type Type = configs::TeeVerifierInputProducerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            l2_chain_id: required(&self.l2_chain_id)
                .and_then(|x| Ok((*x).try_into()?))
                .context("l2_chain_id")?,
            max_attempts: required(&self.max_attempts)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_attempts")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            l2_chain_id: Some(this.l2_chain_id.into()),
            max_attempts: Some(this.max_attempts.into()),
        }
    }
}
