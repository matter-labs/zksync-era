use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::fri_witness_generator as proto;

impl ProtoRepr for proto::FriWitnessGenerator {
    type Type = configs::FriWitnessGeneratorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            generation_timeout_in_secs: required(&self.generation_timeout_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("basic_generation_timeout_in_secs")?,
            basic_generation_timeout_in_secs: self
                .basic_generation_timeout_in_secs
                .map(|x| x.try_into())
                .transpose()
                .context("basic_generation_timeout_in_secs")?,
            leaf_generation_timeout_in_secs: self
                .leaf_generation_timeout_in_secs
                .map(|x| x.try_into())
                .transpose()
                .context("leaf_generation_timeout_in_secs")?,
            node_generation_timeout_in_secs: self
                .node_generation_timeout_in_secs
                .map(|x| x.try_into())
                .transpose()
                .context("node_generation_timeout_in_secs")?,
            scheduler_generation_timeout_in_secs: self
                .scheduler_generation_timeout_in_secs
                .map(|x| x.try_into())
                .transpose()
                .context("scheduler_generation_timeout_in_secs")?,
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
            basic_generation_timeout_in_secs: this
                .basic_generation_timeout_in_secs
                .map(|x| x.into()),
            leaf_generation_timeout_in_secs: this.leaf_generation_timeout_in_secs.map(|x| x.into()),
            node_generation_timeout_in_secs: this.node_generation_timeout_in_secs.map(|x| x.into()),
            scheduler_generation_timeout_in_secs: this
                .scheduler_generation_timeout_in_secs
                .map(|x| x.into()),
            max_attempts: Some(this.max_attempts),
            blocks_proving_percentage: this.blocks_proving_percentage.map(|x| x.into()),
            dump_arguments_for_blocks: this.dump_arguments_for_blocks.clone(),
            last_l1_batch_to_process: this.last_l1_batch_to_process,
            force_process_block: this.force_process_block,
            shall_save_to_public_bucket: Some(this.shall_save_to_public_bucket),
        }
    }
}
