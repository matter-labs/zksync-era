use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto;

impl ProtoRepr for proto::FriWitnessGenerator {
    type Type = configs::FriWitnessGeneratorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            generation_timeout_in_secs: required(&self.generation_timeout_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("generation_timeout_in_secs")?,
            max_attempts: *required(&self.max_attempts).context("max_attempts")?,
            blocks_proving_percentage: self
                .blocks_proving_percentage
                .map(|x| x.try_into())
                .transpose()
                .context("blocks_proving_percentage")?,
            dump_arguments_for_blocks: self.dump_arguments_for_blocks.clone(),
            last_l1_batch_to_process: self.last_l1_batch_to_process,
            force_process_block: self.force_process_block,
            shall_save_to_public_bucket: *required(&self.shall_save_to_public_bucket)
                .context("shall_save_to_public_bucket")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            generation_timeout_in_secs: Some(this.generation_timeout_in_secs.into()),
            max_attempts: Some(this.max_attempts),
            blocks_proving_percentage: this.blocks_proving_percentage.map(|x| x.into()),
            dump_arguments_for_blocks: this.dump_arguments_for_blocks.clone(),
            last_l1_batch_to_process: this.last_l1_batch_to_process,
            force_process_block: this.force_process_block,
            shall_save_to_public_bucket: Some(this.shall_save_to_public_bucket),
        }
    }
}
