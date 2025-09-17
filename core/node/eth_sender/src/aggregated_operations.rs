use std::ops;

use zksync_l1_contract_interface::i_executor::methods::{ExecuteBatches, ProveBatches};
use zksync_types::{
    aggregated_operations::{
        AggregatedActionType, L1BatchAggregatedActionType, L2BlockAggregatedActionType,
    },
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    pubdata_da::PubdataSendingMode,
    transaction_status_commitment::TransactionStatusCommitment,
    L1BatchNumber, L2BlockNumber,
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AggregatedOperation {
    L1Batch(L1BatchAggregatedOperation),
    L2Block(L2BlockAggregatedOperation),
}

#[derive(Debug, Clone)]
pub enum L2BlockAggregatedOperation {
    Precommit {
        l1_batch: L1BatchNumber,
        first_l2_block: L2BlockNumber,
        last_l2_block: L2BlockNumber,
        txs: Vec<TransactionStatusCommitment>,
    },
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
}

impl L2BlockAggregatedOperation {
    pub fn get_action_type(&self) -> L2BlockAggregatedActionType {
        match self {
            Self::Precommit { .. } => L2BlockAggregatedActionType::Precommit,
        }
    }

    pub fn l2_blocks_range(&self) -> ops::RangeInclusive<L2BlockNumber> {
        match self {
            Self::Precommit {
                first_l2_block,
                last_l2_block,
                ..
            } => *first_l2_block..=*last_l2_block,
        }
    }

    pub fn get_action_caption(&self) -> &'static str {
        match self {
            Self::Precommit { .. } => "precommit",
        }
    }
}

impl AggregatedOperation {
    pub fn get_action_type(&self) -> AggregatedActionType {
        match self {
            Self::L1Batch(op) => op.get_action_type().into(),
            Self::L2Block(op) => op.get_action_type().into(),
        }
    }
}
