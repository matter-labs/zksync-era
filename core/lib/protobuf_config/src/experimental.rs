use std::num::NonZeroU32;

use anyhow::Context as _;
use zksync_basic_types::vm::FastVmMode;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::experimental as proto;

impl ProtoRepr for proto::Db {
    type Type = configs::ExperimentalDBConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let state_keeper_db_block_cache_capacity_mb =
            required(&self.state_keeper_db_block_cache_capacity_mb)
                .and_then(|&capacity| Ok(capacity.try_into()?))
                .context("state_keeper_db_block_cache_capacity_mb")?;
        Ok(configs::ExperimentalDBConfig {
            state_keeper_db_block_cache_capacity_mb,
            state_keeper_db_max_open_files: self
                .state_keeper_db_max_open_files
                .map(|count| NonZeroU32::new(count).context("cannot be 0"))
                .transpose()
                .context("state_keeper_db_max_open_files")?,
            protective_reads_persistence_enabled: self
                .reads_persistence_enabled
                .unwrap_or_default(),
            processing_delay_ms: self.processing_delay_ms.unwrap_or_default(),
            include_indices_and_filters_in_block_cache: self
                .include_indices_and_filters_in_block_cache
                .unwrap_or_default(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            state_keeper_db_block_cache_capacity_mb: Some(
                this.state_keeper_db_block_cache_capacity_mb
                    .try_into()
                    .expect("state_keeper_db_block_cache_capacity_mb"),
            ),
            state_keeper_db_max_open_files: this
                .state_keeper_db_max_open_files
                .map(NonZeroU32::get),
            reads_persistence_enabled: Some(this.protective_reads_persistence_enabled),
            processing_delay_ms: Some(this.processing_delay_ms),
            include_indices_and_filters_in_block_cache: Some(
                this.include_indices_and_filters_in_block_cache,
            ),
        }
    }
}

impl proto::FastVmMode {
    fn new(source: configs::vm_runner::FastVmMode) -> Self {
        use configs::vm_runner::FastVmMode as From;
        match source {
            From::Old => Self::Old,
            From::New => Self::New,
            From::Shadow => Self::Shadow,
        }
    }

    fn parse(&self) -> configs::vm_runner::FastVmMode {
        use configs::vm_runner::FastVmMode as To;
        match self {
            Self::Old => To::Old,
            Self::New => To::New,
            Self::Shadow => To::Shadow,
        }
    }
}

impl ProtoRepr for proto::Vm {
    type Type = configs::ExperimentalVmConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            fast_vm_mode: self
                .fast_vm_mode
                .map(proto::FastVmMode::try_from)
                .transpose()?
                .map_or_else(FastVmMode::default, |mode| mode.parse()),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            fast_vm_mode: Some(proto::FastVmMode::new(this.fast_vm_mode).into()),
        }
    }
}
