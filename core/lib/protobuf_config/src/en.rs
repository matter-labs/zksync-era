use std::{num::NonZeroUsize, str::FromStr};

use anyhow::Context;
use zksync_basic_types::{url::SensitiveUrl, L1ChainId, L2ChainId};
use zksync_config::configs::en_config::ENConfig;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::en as proto;

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
            main_node_rate_limit_rps: self
                .main_node_rate_limit_rps
                .and_then(|a| NonZeroUsize::new(a as usize)),
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
            main_node_rate_limit_rps: this.main_node_rate_limit_rps.map(|a| a.get() as u32),
        }
    }
}
