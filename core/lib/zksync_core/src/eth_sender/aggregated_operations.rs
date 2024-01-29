use std::ops;

use zksync_prover_interface::outputs::L1BatchProofForL1;
use zksync_types::{
    aggregated_operations::AggregatedActionType, commitment::L1BatchWithMetadata, L1BatchNumber,
    ProtocolVersionId,
};

fn l1_batch_range_from_batches(
    batches: &[L1BatchWithMetadata],
) -> ops::RangeInclusive<L1BatchNumber> {
    let start = batches
        .first()
        .map(|l1_batch| l1_batch.header.number)
        .unwrap_or_default();
    let end = batches
        .last()
        .map(|l1_batch| l1_batch.header.number)
        .unwrap_or_default();
    start..=end
}

#[derive(Debug, Clone)]
pub struct L1BatchCommitOperation {
    pub last_committed_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl L1BatchCommitOperation {
    pub fn l1_batch_range(&self) -> ops::RangeInclusive<L1BatchNumber> {
        l1_batch_range_from_batches(&self.l1_batches)
    }
}

#[derive(Debug, Clone)]
pub struct L1BatchProofOperation {
    pub prev_l1_batch: L1BatchWithMetadata,
    pub l1_batches: Vec<L1BatchWithMetadata>,
    pub proofs: Vec<L1BatchProofForL1>,
    pub should_verify: bool,
}

impl L1BatchProofOperation {
    pub fn l1_batch_range(&self) -> ops::RangeInclusive<L1BatchNumber> {
        l1_batch_range_from_batches(&self.l1_batches)
    }
}

#[derive(Debug, Clone)]
pub struct L1BatchExecuteOperation {
    pub l1_batches: Vec<L1BatchWithMetadata>,
}

impl L1BatchExecuteOperation {
    pub fn l1_batch_range(&self) -> ops::RangeInclusive<L1BatchNumber> {
        l1_batch_range_from_batches(&self.l1_batches)
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AggregatedOperation {
    Commit(L1BatchCommitOperation),
    PublishProofOnchain(L1BatchProofOperation),
    Execute(L1BatchExecuteOperation),
}

impl AggregatedOperation {
    pub fn get_action_type(&self) -> AggregatedActionType {
        match self {
            Self::Commit(_) => AggregatedActionType::Commit,
            Self::PublishProofOnchain(_) => AggregatedActionType::PublishProofOnchain,
            Self::Execute(_) => AggregatedActionType::Execute,
        }
    }

    pub fn l1_batch_range(&self) -> ops::RangeInclusive<L1BatchNumber> {
        match self {
            Self::Commit(op) => op.l1_batch_range(),
            Self::PublishProofOnchain(op) => op.l1_batch_range(),
            Self::Execute(op) => op.l1_batch_range(),
        }
    }

    pub fn get_action_caption(&self) -> &'static str {
        match self {
            Self::Commit(_) => "commit",
            Self::PublishProofOnchain(_) => "proof",
            Self::Execute(_) => "execute",
        }
    }

    pub fn protocol_version(&self) -> ProtocolVersionId {
        match self {
            Self::Commit(op) => op.l1_batches[0].header.protocol_version.unwrap(),
            Self::PublishProofOnchain(op) => op.l1_batches[0].header.protocol_version.unwrap(),
            Self::Execute(op) => op.l1_batches[0].header.protocol_version.unwrap(),
        }
    }
}
