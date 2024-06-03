use std::{
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    str::FromStr,
};

use anyhow::Context;
use zksync_basic_types::{url::SensitiveUrl, L1ChainId, L2ChainId};
use zksync_config::configs::en_config::{ENConfig, Pruning, SnapshotRecovery};
use zksync_protobuf::{required, ProtoRepr};

use crate::{proto::en as proto, read_optional_repr};

impl ProtoRepr for proto::Pruning {
    type Type = Pruning;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            enabled: self.enabled.unwrap_or_default(),
            chunk_size: self.chunk_size,
            removal_delay_sec: self.removal_delay_sec.and_then(NonZeroU64::new),
            data_retention_sec: self.data_retention_sec,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            enabled: Some(this.enabled),
            chunk_size: this.chunk_size,
            removal_delay_sec: this.removal_delay_sec.map(|a| a.get()),
            data_retention_sec: this.data_retention_sec,
        }
    }
}

impl ProtoRepr for proto::SnapshotRecovery {
    type Type = SnapshotRecovery;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            enabled: self.enabled.unwrap_or_default(),
            postgres_max_concurrency: self
                .postgres_max_concurrency
                .and_then(|a| NonZeroUsize::new(a as usize)),
            tree_chunk_size: self.tree_chunk_size,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            enabled: Some(this.enabled),
            postgres_max_concurrency: this.postgres_max_concurrency.map(|a| a.get() as u64),
            tree_chunk_size: this.tree_chunk_size,
        }
    }
}

impl ProtoRepr for proto::ExternalNode {
    type Type = ENConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            main_node_url: SensitiveUrl::from_str(
                required(&self.main_node_url).context("main_node_url")?,
            )?,
            l1_chain_id: required(&self.l1_chain_id)
                .map(|x| L1ChainId(*x))
                .context("l1_chain_id")?,
            l2_chain_id: required(&self.l2_chain_id)
                .and_then(|x| L2ChainId::try_from(*x).map_err(|a| anyhow::anyhow!(a)))
                .context("l2_chain_id")?,
            l1_batch_commit_data_generator_mode: required(
                &self.l1_batch_commit_data_generator_mode,
            )
            .and_then(|x| Ok(crate::proto::genesis::L1BatchCommitDataGeneratorMode::try_from(*x)?))
            .context("l1_batch_commit_data_generator_mode")?
            .parse(),
            commitment_generator_max_parallelism: self
                .commitment_generator_max_parallelism
                .and_then(NonZeroU32::new),
            tree_api_remote_url: self.tree_api_remote_url.clone(),
            main_node_rate_limit_rps: self
                .main_node_rate_limit_rps
                .and_then(|a| NonZeroUsize::new(a as usize)),
            pruning: read_optional_repr(&self.pruning).context("pruning")?,
            snapshot_recovery: read_optional_repr(&self.snapshot_recovery)
                .context("snapshot_recovery")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            main_node_url: Some(this.main_node_url.expose_str().to_string()),
            l1_chain_id: Some(this.l1_chain_id.0),
            l2_chain_id: Some(this.l2_chain_id.as_u64()),
            l1_batch_commit_data_generator_mode: Some(
                crate::proto::genesis::L1BatchCommitDataGeneratorMode::new(
                    &this.l1_batch_commit_data_generator_mode,
                )
                .into(),
            ),
            tree_api_remote_url: this.tree_api_remote_url.clone(),
            commitment_generator_max_parallelism: this
                .commitment_generator_max_parallelism
                .map(|a| a.get()),
            main_node_rate_limit_rps: this.main_node_rate_limit_rps.map(|a| a.get() as u32),
            snapshot_recovery: this.snapshot_recovery.as_ref().map(ProtoRepr::build),
            pruning: this.pruning.as_ref().map(ProtoRepr::build),
        }
    }
}
