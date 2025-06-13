use zk_ee::system::errors::InternalError;
use zk_os_forward_system::run::{BatchContext, BatchOutput, InvalidTransaction};
use zksync_types::Transaction;
use zksync_zkos_vm_runner::zkos_conversions::tx_abi_encode;
use crate::BLOCK_TIME_MS;
use crate::execution::vm_wrapper::VmWrapper;
use crate::mempool::Mempool;
use crate::storage::block_replay_wal::ReplayRecord;
use crate::storage::StateHandle;

// #[derive(Clone, Debug)]
// pub struct BlockExecutor {
//     mempool: Mempool<Vec<u8>>,
//     state_handle: StateHandle,
// }

// impl BlockExecutor {
//     pub fn new(
//     ) -> Self {
//         BlockExecutor {
//             mempool,
//             state_handle,
//         }
//     }

/// Executes a block of transactions without any mutations to state
pub async fn execute_block(
    block_context: BatchContext,
    mempool: Mempool,
    state_handle: StateHandle,
) -> (BatchOutput, ReplayRecord) {
    tracing::info!("starting executing block number: {}", block_context.block_number);
    let state_view = state_handle.0.in_memory_storage.view_at(block_context.block_number);
    let preimages = state_handle.0.in_memory_preimages.clone();
    tracing::info!("Prepared state view");

    let mut runner = VmWrapper::new(
        block_context.clone(),
        state_view,
        preimages,
    );

    // todo: other seal criteria

    let mut executed_transactions: Vec<Transaction> = Vec::new();

    let started_at = std::time::Instant::now();
    let mut waited = false;
    loop {
        if !executed_transactions.is_empty() && started_at.elapsed().as_millis() >= BLOCK_TIME_MS {
            tracing::info!("Block {} deadline - sealing", block_context.block_number);
            break;
        }
        let tx = match mempool.get_next() {
            None => {
                // No more transactions in the mempool, waiting
                if !waited {
                    tracing::info!("Block {} No transactions in mempool, waiting for new ones", block_context.block_number);
                    waited = true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                continue;
            }
            Some(tx) => tx
        };
        let started_tx = std::time::Instant::now();
        executed_transactions.push(tx.clone());
        match runner.execute_next_tx(tx_abi_encode(tx)).await {
            Ok(tx_out) => {
                tracing::trace!(
                        "Executed tx in block {}: {:?} - after {:?}",
                        block_context.block_number,
                        tx_out,
                        started_tx.elapsed()
                    );
            }
            Err(InvalidTransaction::NonceTooHigh {tx, state}) => {
                tracing::warn!(
                        "Nonce too high in block {} tx: {} state: {}",
                        block_context.block_number,
                        tx,
                        state
                    );
            }
            Err(invalid) => {
                tracing::warn!(
                        "Invalid transaction in block {}: {:?} - after {:?}",
                        block_context.block_number,
                        invalid,
                        started_tx.elapsed()
                    );
                break;
            }
        }
    }

    tracing::info!(
        "Sealing block {} after {:?} with {} executed transactions",
        block_context.block_number,
        started_at.elapsed(),
        executed_transactions.len(),
    );

    // todo: handle errors within the main stream
    let block_output = runner.seal_batch().await
        .expect("Failed to finish block in VM");
    let replay_data = ReplayRecord {
        context: block_context,
        transactions: executed_transactions,
    };
    (
        block_output,
        replay_data
    )
}