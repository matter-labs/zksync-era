use anyhow::Context as _;
use zksync_basic_types::{commitment::L1BatchCommitmentMode, L1ChainId, L2ChainId};
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::{parse_h160, parse_h256, proto::genesis as proto};

impl proto::L1BatchCommitDataGeneratorMode {
    fn new(n: &L1BatchCommitmentMode) -> Self {
        match n {
            L1BatchCommitmentMode::Rollup => Self::Rollup,
            L1BatchCommitmentMode::Validium => Self::Validium,
        }
    }

    fn parse(&self) -> L1BatchCommitmentMode {
        match self {
            Self::Rollup => L1BatchCommitmentMode::Rollup,
            Self::Validium => L1BatchCommitmentMode::Validium,
        }
    }
}
impl ProtoRepr for proto::Genesis {
    type Type = configs::GenesisConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let prover = required(&self.prover).context("prover")?;
        Ok(Self::Type {
            protocol_version: Some(
                required(&self.genesis_protocol_version)
                    .map(|x| *x as u16)
                    .context("protocol_version")?,
            ),
            genesis_root_hash: Some(
                required(&self.genesis_root)
                    .and_then(|x| parse_h256(x))
                    .context("genesis_root_hash")?,
            ),
            rollup_last_leaf_index: Some(
                *required(&self.genesis_rollup_leaf_index).context("rollup_last_leaf_index")?,
            ),
            genesis_commitment: Some(
                required(&self.genesis_batch_commitment)
                    .and_then(|x| parse_h256(x))
                    .context("genesis_commitment")?,
            ),
            bootloader_hash: Some(
                required(&self.bootloader_hash)
                    .and_then(|x| parse_h256(x))
                    .context("bootloader_hash")?,
            ),
            default_aa_hash: Some(
                required(&self.default_aa_hash)
                    .and_then(|x| parse_h256(x))
                    .context("default_aa_hash")?,
            ),
            l1_chain_id: required(&self.l1_chain_id)
                .map(|x| L1ChainId(*x))
                .context("l1_chain_id")?,
            l2_chain_id: required(&self.l2_chain_id)
                .and_then(|x| L2ChainId::try_from(*x).map_err(|a| anyhow::anyhow!(a)))
                .context("l2_chain_id")?,
            recursion_node_level_vk_hash: required(&prover.recursion_node_level_vk_hash)
                .and_then(|x| parse_h256(x))
                .context("recursion_node_level_vk_hash")?,
            recursion_leaf_level_vk_hash: required(&prover.recursion_leaf_level_vk_hash)
                .and_then(|x| parse_h256(x))
                .context("recursion_leaf_level_vk_hash")?,
            recursion_circuits_set_vks_hash: prover
                .recursion_circuits_set_vks_hash
                .as_ref()
                .map(|x| parse_h256(x))
                .transpose()
                .context("recursion_circuits_set_vks_hash")?
                .unwrap_or_default(),
            recursion_scheduler_level_vk_hash: required(&prover.recursion_scheduler_level_vk_hash)
                .and_then(|x| parse_h256(x))
                .context("recursion_scheduler_level_vk_hash")?,
            fee_account: required(&self.fee_account)
                .and_then(|x| parse_h160(x))
                .context("fee_account")?,
            dummy_verifier: *required(&prover.dummy_verifier).context("dummy_verifier")?,
            l1_batch_commit_data_generator_mode: required(
                &self.l1_batch_commit_data_generator_mode,
            )
            .and_then(|x| Ok(proto::L1BatchCommitDataGeneratorMode::try_from(*x)?))
            .context("l1_batch_commit_data_generator_mode")?
            .parse(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            genesis_root: this.genesis_root_hash.map(|x| format!("{:?}", x)),
            genesis_rollup_leaf_index: this.rollup_last_leaf_index,
            genesis_batch_commitment: this.genesis_commitment.map(|x| format!("{:?}", x)),
            genesis_protocol_version: this.protocol_version.map(|x| x as u32),
            default_aa_hash: this.default_aa_hash.map(|x| format!("{:?}", x)),
            bootloader_hash: this.bootloader_hash.map(|x| format!("{:?}", x)),
            fee_account: Some(format!("{:?}", this.fee_account)),
            l1_chain_id: Some(this.l1_chain_id.0),
            l2_chain_id: Some(this.l2_chain_id.as_u64()),
            prover: Some(proto::Prover {
                recursion_scheduler_level_vk_hash: Some(format!(
                    "{:?}",
                    this.recursion_scheduler_level_vk_hash
                )),
                recursion_node_level_vk_hash: Some(format!(
                    "{:?}",
                    this.recursion_node_level_vk_hash
                )),
                recursion_leaf_level_vk_hash: Some(format!(
                    "{:?}",
                    this.recursion_leaf_level_vk_hash
                )),
                recursion_circuits_set_vks_hash: Some(format!(
                    "{:?}",
                    this.recursion_circuits_set_vks_hash
                )),
                dummy_verifier: Some(this.dummy_verifier),
            }),
            l1_batch_commit_data_generator_mode: Some(
                proto::L1BatchCommitDataGeneratorMode::new(
                    &this.l1_batch_commit_data_generator_mode,
                )
                .into(),
            ),
        }
    }
}
