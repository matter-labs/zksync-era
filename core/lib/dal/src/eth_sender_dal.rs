use crate::models::storage_eth_tx::{StorageEthTx, StorageTxHistory, StorageTxHistoryToSend};
use crate::StorageProcessor;
use std::convert::TryFrom;
use zksync_types::aggregated_operations::AggregatedActionType;
use zksync_types::eth_sender::{EthTx, TxHistory, TxHistoryToSend};
use zksync_types::{Address, H256, U256};

#[derive(Debug)]
pub struct EthSenderDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl EthSenderDal<'_, '_> {
    pub fn get_inflight_txs(&mut self) -> Vec<EthTx> {
        async_std::task::block_on(async {
            let txs = sqlx::query_as!(
                StorageEthTx,
                "SELECT * FROM eth_txs WHERE confirmed_eth_tx_history_id IS NULL 
                 AND id <= (SELECT COALESCE(MAX(eth_tx_id), 0) FROM eth_txs_history WHERE sent_at_block IS NOT NULL)
                 ORDER BY id"
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            txs.into_iter().map(|tx| tx.into()).collect()
        })
    }

    pub fn get_eth_tx(&mut self, eth_tx_id: u32) -> Option<EthTx> {
        async_std::task::block_on(async {
            sqlx::query_as!(
                StorageEthTx,
                "SELECT * FROM eth_txs WHERE id = $1",
                eth_tx_id as i32
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .map(Into::into)
        })
    }

    pub fn get_new_eth_txs(&mut self, limit: u64) -> Vec<EthTx> {
        async_std::task::block_on(async {
            let txs = sqlx::query_as!(
                StorageEthTx,
                r#"SELECT * FROM eth_txs 
                   WHERE id > (SELECT COALESCE(MAX(eth_tx_id), 0) FROM eth_txs_history)
                   ORDER BY id
                   LIMIT $1
                   "#,
                limit as i64
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            txs.into_iter().map(|tx| tx.into()).collect()
        })
    }

    pub fn get_unsent_txs(&mut self) -> Vec<TxHistoryToSend> {
        async_std::task::block_on(async {
            let txs = sqlx::query_as!(
                StorageTxHistoryToSend,
                r#"
                SELECT 
                    eth_txs_history.id,
                    eth_txs_history.eth_tx_id,
                    eth_txs_history.tx_hash,
                    eth_txs_history.base_fee_per_gas,
                    eth_txs_history.priority_fee_per_gas,
                    eth_txs_history.signed_raw_tx,
                    eth_txs.nonce
                FROM eth_txs_history 
                JOIN eth_txs ON eth_txs.id = eth_txs_history.eth_tx_id 
                WHERE eth_txs_history.sent_at_block IS NULL AND eth_txs.confirmed_eth_tx_history_id IS NULL
                ORDER BY eth_txs_history.id DESC"#,
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            txs.into_iter().map(|tx| tx.into()).collect()
        })
    }

    pub fn save_eth_tx(
        &mut self,
        nonce: u64,
        raw_tx: Vec<u8>,
        tx_type: AggregatedActionType,
        contract_address: Address,
        predicted_gas_cost: u32,
    ) -> EthTx {
        async_std::task::block_on(async {
            let address = format!("{:#x}", contract_address);
            let eth_tx = sqlx::query_as!(
            StorageEthTx,
            "INSERT INTO eth_txs (raw_tx, nonce, tx_type, contract_address, predicted_gas_cost, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, now(), now())
               RETURNING *",
            raw_tx,
            nonce as i64,
            tx_type.to_string(),
            address,
            predicted_gas_cost as i64
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();
            eth_tx.into()
        })
    }

    pub fn insert_tx_history(
        &mut self,
        eth_tx_id: u32,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
        tx_hash: H256,
        raw_signed_tx: Vec<u8>,
    ) -> u32 {
        async_std::task::block_on(async {
            let priority_fee_per_gas =
                i64::try_from(priority_fee_per_gas).expect("Can't convert U256 to i64");
            let base_fee_per_gas =
                i64::try_from(base_fee_per_gas).expect("Can't convert U256 to i64");
            let tx_hash = format!("{:#x}", tx_hash);

            sqlx::query!(
                "INSERT INTO eth_txs_history
                (eth_tx_id, base_fee_per_gas, priority_fee_per_gas, tx_hash, signed_raw_tx, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, now(), now())
                RETURNING id",
                eth_tx_id as u32,
                base_fee_per_gas,
                priority_fee_per_gas,
                tx_hash,
                raw_signed_tx
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .id as u32
        })
    }

    pub fn set_sent_at_block(&mut self, eth_txs_history_id: u32, sent_at_block: u32) {
        async_std::task::block_on(async {
            sqlx::query!(
                "UPDATE eth_txs_history SET sent_at_block = $2, sent_at = now()
                WHERE id = $1 AND sent_at_block IS NULL",
                eth_txs_history_id as i32,
                sent_at_block as i32
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn remove_tx_history(&mut self, eth_txs_history_id: u32) {
        async_std::task::block_on(async {
            sqlx::query!(
                "DELETE FROM eth_txs_history
                WHERE id = $1",
                eth_txs_history_id as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn confirm_tx(&mut self, tx_hash: H256, gas_used: U256) {
        async_std::task::block_on(async {
            let gas_used = i64::try_from(gas_used).expect("Can't convert U256 to i64");
            let tx_hash = format!("{:#x}", tx_hash);
            let ids = sqlx::query!(
                "UPDATE eth_txs_history
                SET updated_at = now(), confirmed_at = now()
                WHERE tx_hash = $1
                RETURNING id, eth_tx_id",
                tx_hash,
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap();

            sqlx::query!(
                "UPDATE eth_txs
                SET gas_used = $1, confirmed_eth_tx_history_id = $2
                WHERE id = $3",
                gas_used,
                ids.id,
                ids.eth_tx_id
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn get_tx_history_to_check(&mut self, eth_tx_id: u32) -> Vec<TxHistory> {
        async_std::task::block_on(async {
            let tx_history = sqlx::query_as!(
                StorageTxHistory,
                "SELECT * FROM eth_txs_history WHERE eth_tx_id = $1 ORDER BY created_at DESC",
                eth_tx_id as i32
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            tx_history.into_iter().map(|tx| tx.into()).collect()
        })
    }

    pub fn get_block_number_on_first_sent_attempt(&mut self, eth_tx_id: u32) -> Option<u32> {
        async_std::task::block_on(async {
            let sent_at_block = sqlx::query_scalar!(
            "SELECT sent_at_block FROM eth_txs_history WHERE eth_tx_id = $1 AND sent_at_block IS NOT NULL ORDER BY created_at ASC LIMIT 1",
            eth_tx_id as i32
        )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap();
            sent_at_block.flatten().map(|block| block as u32)
        })
    }

    pub fn get_last_sent_eth_tx(&mut self, eth_tx_id: u32) -> Option<TxHistory> {
        async_std::task::block_on(async {
            let history_item = sqlx::query_as!(
            StorageTxHistory,
            "SELECT * FROM eth_txs_history WHERE eth_tx_id = $1 ORDER BY created_at DESC LIMIT 1",
            eth_tx_id as i32
        )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap();
            history_item.map(|tx| tx.into())
        })
    }

    pub fn get_next_nonce(&mut self) -> Option<u64> {
        async_std::task::block_on(async {
            sqlx::query!(r#"SELECT MAX(nonce) as "max_nonce?" FROM eth_txs"#,)
                .fetch_one(self.storage.conn())
                .await
                .unwrap()
                .max_nonce
                .map(|n| n as u64 + 1)
        })
    }

    pub fn mark_failed_transaction(&mut self, eth_tx_id: u32) {
        async_std::task::block_on(async {
            sqlx::query!(
                "UPDATE eth_txs SET has_failed = TRUE WHERE id = $1",
                eth_tx_id as i32
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn get_number_of_failed_transactions(&mut self) -> i64 {
        async_std::task::block_on(async {
            sqlx::query!("SELECT COUNT(*) FROM eth_txs WHERE has_failed = TRUE")
                .fetch_one(self.storage.conn())
                .await
                .unwrap()
                .count
                .unwrap()
        })
    }

    pub fn clear_failed_transactions(&mut self) {
        async_std::task::block_on(async {
            sqlx::query!(
                "DELETE FROM eth_txs WHERE id >=
                (SELECT MIN(id) FROM eth_txs WHERE has_failed = TRUE)"
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }
}
