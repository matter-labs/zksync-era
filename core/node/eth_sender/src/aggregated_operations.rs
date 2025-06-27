use std::ops;

use zksync_l1_contract_interface::i_executor::methods::{ExecuteBatches, ProveBatches};
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    pubdata_da::PubdataSendingMode,
    L1BatchNumber, ProtocolVersionId,
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AggregatedOperation {
    Commit(
        L1BatchWithMetadata,
        Vec<L1BatchWithMetadata>,
        PubdataSendingMode,
        L1BatchCommitmentMode,
    ),
    PublishProofOnchain(ProveBatches),
    Execute(ExecuteBatches),
}

impl AggregatedOperation {
    pub fn get_action_type(&self) -> AggregatedActionType {
        match self {
            Self::Commit(..) => AggregatedActionType::Commit,
            Self::PublishProofOnchain(_) => AggregatedActionType::PublishProofOnchain,
            Self::Execute(_) => AggregatedActionType::Execute,
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

    pub fn dependency_roots_per_batch(&self) -> Vec<u64> {
        match self {
            Self::Execute(op) => op
                .dependency_roots
                .iter()
                .map(|roots| roots.len() as u64)
                .collect(),
            _ => vec![],
        }
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
