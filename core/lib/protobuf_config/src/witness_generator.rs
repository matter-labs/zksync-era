use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::required;

use crate::{proto, repr::ProtoRepr};

impl proto::BasicWitnessGeneratorDataSource {
    fn new(x: &configs::witness_generator::BasicWitnessGeneratorDataSource) -> Self {
        type From = configs::witness_generator::BasicWitnessGeneratorDataSource;
        match x {
            From::FromPostgres => Self::FromPostgres,
            From::FromPostgresShadowBlob => Self::FromPostgresShadowBlob,
            From::FromBlob => Self::FromBlob,
        }
    }
    fn parse(&self) -> configs::witness_generator::BasicWitnessGeneratorDataSource {
        type To = configs::witness_generator::BasicWitnessGeneratorDataSource;
        match self {
            Self::FromPostgres => To::FromPostgres,
            Self::FromPostgresShadowBlob => To::FromPostgresShadowBlob,
            Self::FromBlob => To::FromBlob,
        }
    }
}

impl ProtoRepr for proto::WitnessGenerator {
    type Type = configs::WitnessGeneratorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            generation_timeout_in_secs: required(&self.generation_timeout_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("generation_timeout_in_secs")?,
            initial_setup_key_path: required(&self.initial_setup_key_path)
                .context("initial_setup_key_path")?
                .clone(),
            key_download_url: required(&self.key_download_url)
                .context("key_download_url")?
                .clone(),
            max_attempts: *required(&self.max_attempts).context("max_attempts")?,
            blocks_proving_percentage: self
                .blocks_proving_percentage
                .map(|x| x.try_into())
                .transpose()
                .context("blocks_proving_percentage")?,
            dump_arguments_for_blocks: self.dump_arguments_for_blocks.clone(),
            last_l1_batch_to_process: self.last_l1_batch_to_process,
            data_source: required(&self.data_source)
                .and_then(|x| Ok(proto::BasicWitnessGeneratorDataSource::try_from(*x)?))
                .context("data_source")?
                .parse(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            generation_timeout_in_secs: Some(this.generation_timeout_in_secs.into()),
            initial_setup_key_path: Some(this.initial_setup_key_path.clone()),
            key_download_url: Some(this.key_download_url.clone()),
            max_attempts: Some(this.max_attempts),
            blocks_proving_percentage: this.blocks_proving_percentage.map(|x| x.into()),
            dump_arguments_for_blocks: this.dump_arguments_for_blocks.clone(),
            last_l1_batch_to_process: this.last_l1_batch_to_process,
            data_source: Some(
                proto::BasicWitnessGeneratorDataSource::new(&this.data_source).into(),
            ),
        }
    }
}
