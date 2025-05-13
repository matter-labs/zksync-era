use std::ops;

use zksync_l1_contract_interface::i_executor::methods::{ExecuteBatches, ProveBatches};
use zksync_types::aggregated_operations::{AggregatedActionType, MiniblockAggregatedActionType};
use zksync_types::{
    aggregated_operations::L1BatchAggregatedActionType,
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    pubdata_da::PubdataSendingMode,
    L1BatchNumber, L2BlockNumber, ProtocolVersionId, H256,
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AggregatedOperation {
    L1BatchAggregatedOperation(L1BatchAggregatedOperation),
    MiniblockAggregatedOperation(MiniblockAggregatedOperation),
}

#[derive(Debug, Clone)]
pub enum MiniblockAggregatedOperation {
    PreCommit(L1BatchNumber, L2BlockNumber, H256),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum L1BatchAggregatedOperation {
    Commit(
        L1BatchWithMetadata,
        Vec<L1BatchWithMetadata>,
        PubdataSendingMode,
        L1BatchCommitmentMode,
    ),
    PublishProofOnchain(ProveBatches),
    Execute(ExecuteBatches),
}

impl L1BatchAggregatedOperation {
    pub fn get_action_type(&self) -> L1BatchAggregatedActionType {
        match self {
            Self::Commit(..) => L1BatchAggregatedActionType::Commit,
            Self::PublishProofOnchain(_) => L1BatchAggregatedActionType::PublishProofOnchain,
            Self::Execute(_) => L1BatchAggregatedActionType::Execute,
        }
    }

    pub fn l1_batch_range(&self) -> ops::RangeInclusive<L1BatchNumber> {
        let batches = match self {
            Self::Commit(_, l1_batches, _, _) => l1_batches,
            Self::PublishProofOnchain(op) => &op.l1_batches,
            Self::Execute(op) => &op.l1_batches,
        };

        if batches.is_empty() {
            return L1BatchNumber(0)..=L1BatchNumber(0);
        }
        let first_batch = &batches[0];
        let last_batch = &batches[batches.len() - 1];
        first_batch.header.number..=last_batch.header.number
    }

    pub fn get_action_caption(&self) -> &'static str {
        match self {
            Self::Commit(..) => "commit",
            Self::PublishProofOnchain(_) => "proof",
            Self::Execute(_) => "execute",
        }
    }

    pub fn protocol_version(&self) -> ProtocolVersionId {
        match self {
            Self::Commit(_, l1_batches, _, _) => l1_batches[0].header.protocol_version.unwrap(),
            Self::PublishProofOnchain(op) => op.l1_batches[0].header.protocol_version.unwrap(),
            Self::Execute(op) => op.l1_batches[0].header.protocol_version.unwrap(),
        }
    }
}

impl MiniblockAggregatedOperation {
    pub fn get_action_type(&self) -> MiniblockAggregatedActionType {
        match self {
            Self::PreCommit(..) => MiniblockAggregatedActionType::PreCommit,
        }
    }

    pub fn get_action_caption(&self) -> &'static str {
        match self {
            Self::PreCommit(_, _, _) => "pre_commit",
        }
    }
}

impl AggregatedOperation {
    pub fn get_action_type(&self) -> AggregatedActionType {
        match self {
            Self::L1BatchAggregatedOperation(op) => op.get_action_type().into(),
            Self::MiniblockAggregatedOperation(op) => op.get_action_type().into(),
        }
    }

    pub fn get_action_caption(&self) -> &'static str {
        match self {
            Self::L1BatchAggregatedOperation(op) => op.get_action_caption(),
            Self::MiniblockAggregatedOperation(op) => op.get_action_caption(),
        }
    }
}
