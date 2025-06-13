use std::fmt::Display;
use serde::Deserialize;
use tracing::Value;
use zk_os_forward_system::run::BatchContext;
use zksync_types::Transaction;
use zksync_web3_decl::jsonrpsee::core::Serialize;

#[derive(Clone, Debug)]
pub enum BlockCommand {
    /// Replay a block from the WAL.
    Replay(ReplayRecord),
    /// Produce a new block from the mempool.
    Produce(BatchContext),
}

impl BlockCommand {
    pub fn block_number(&self) -> u64 {
        match self {
            BlockCommand::Replay(record) => record.context.block_number,
            BlockCommand::Produce(context) => context.block_number,
        }
    }
}

/// Full data needed to replay a block - assuming storage is already in the correct state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayRecord {
    pub context: BatchContext,
    pub transactions: Vec<Transaction>,
}


// todo: get rid/refactor `Transaction` type in zksync-types crate
pub enum TransactionSource {
    // todo: use actual type for transactions
    Replay(Vec<Transaction>),
    Mempool,
}

impl Display for BlockCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockCommand::Replay(record) => write!(f, "Replay block {} ({} txs)", record.context.block_number, record.transactions.len()),
            BlockCommand::Produce(context) => write!(f, "Produce block {}", context.block_number),
        }
    }
}