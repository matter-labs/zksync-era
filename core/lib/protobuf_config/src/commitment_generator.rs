use std::num::NonZeroU32;

use anyhow::Context as _;
use zksync_config::configs::CommitmentGeneratorConfig;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::commitment_generator as proto;

impl ProtoRepr for proto::CommitmentGenerator {
    type Type = CommitmentGeneratorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            max_parallelism: NonZeroU32::new(
                *required(&self.max_parallelism).context("max_parallelism")?,
            )
            .context("cannot be 0")?,
        })
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            max_parallelism: Some(this.max_parallelism.into()),
        }
    }
}
