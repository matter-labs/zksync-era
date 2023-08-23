use zksync_types::{api::en::SyncBlock, Address, MiniblockNumber, Transaction};

use crate::{
    instrument::{InstrumentExt, MethodLatency},
    models::{storage_sync::StorageSyncBlock, storage_transaction::StorageTransaction},
    SqlxError, StorageProcessor,
};

/// DAL subset dedicated to the EN synchronization.
#[derive(Debug)]
pub struct SyncDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl SyncDal<'_, '_> {
    pub async fn sync_block(
        &mut self,
        block_number: MiniblockNumber,
        current_operator_address: Address,
        include_transactions: bool,
    ) -> Result<Option<SyncBlock>, SqlxError> {
        let latency = MethodLatency::new("sync_dal_sync_block");
        let storage_block_details = sqlx::query_as!(
            StorageSyncBlock,
            r#"
                SELECT miniblocks.number,
                    COALESCE(miniblocks.l1_batch_number, (SELECT (max(number) + 1) FROM l1_batches)) as "l1_batch_number!",
                    (SELECT max(m2.number) FROM miniblocks m2 WHERE miniblocks.l1_batch_number = m2.l1_batch_number) as "last_batch_miniblock?",
                    miniblocks.timestamp,
                    miniblocks.hash as "root_hash?",
                    commit_tx.tx_hash as "commit_tx_hash?",
                    commit_tx.confirmed_at as "committed_at?",
                    prove_tx.tx_hash as "prove_tx_hash?",
                    prove_tx.confirmed_at as "proven_at?",
                    execute_tx.tx_hash as "execute_tx_hash?",
                    execute_tx.confirmed_at as "executed_at?",
                    miniblocks.l1_gas_price,
                    miniblocks.l2_fair_gas_price,
                    miniblocks.bootloader_code_hash,
                    miniblocks.default_aa_code_hash,
                    l1_batches.fee_account_address as "fee_account_address?"
                FROM miniblocks
                LEFT JOIN l1_batches ON miniblocks.l1_batch_number = l1_batches.number
                LEFT JOIN eth_txs_history as commit_tx ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id AND commit_tx.confirmed_at IS NOT NULL)
                LEFT JOIN eth_txs_history as prove_tx ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id AND prove_tx.confirmed_at IS NOT NULL)
                LEFT JOIN eth_txs_history as execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id AND execute_tx.confirmed_at IS NOT NULL)
                WHERE miniblocks.number = $1
            "#,
            block_number.0 as i64
        )
        .instrument("sync_dal_sync_block.block")
        .with_arg("block_number", &block_number)
        .fetch_optional(self.storage.conn())
        .await?;

        let res = if let Some(storage_block_details) = storage_block_details {
            let transactions = if include_transactions {
                let block_transactions = sqlx::query_as!(
                    StorageTransaction,
                    r#"SELECT * FROM transactions WHERE miniblock_number = $1 ORDER BY index_in_block"#,
                    block_number.0 as i64
                )
                .instrument("sync_dal_sync_block.transactions")
                .with_arg("block_number", &block_number)
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .map(Transaction::from)
                .collect();
                Some(block_transactions)
            } else {
                None
            };
            Some(storage_block_details.into_sync_block(current_operator_address, transactions))
        } else {
            None
        };

        drop(latency);
        Ok(res)
    }
}
