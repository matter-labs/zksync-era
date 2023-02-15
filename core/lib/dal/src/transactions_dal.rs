use bigdecimal::BigDecimal;
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::iter::FromIterator;
use std::time::Duration;
use zksync_types::fee::TransactionExecutionMetrics;

use itertools::Itertools;
use sqlx::error;
use sqlx::types::chrono::NaiveDateTime;

use zksync_types::tx::tx_execution_info::TxExecutionStatus;
use zksync_types::{get_nonce_key, U256};
use zksync_types::{
    l1::L1Tx, l2::L2Tx, tx::TransactionExecutionResult, vm_trace::VmExecutionTrace, Address,
    ExecuteTransactionCommon, L1BatchNumber, L1BlockNumber, MiniblockNumber, Nonce, PriorityOpId,
    Transaction, H256,
};
use zksync_utils::{h256_to_u32, u256_to_big_decimal};

use crate::models::storage_transaction::StorageTransaction;
use crate::time_utils::pg_interval_from_duration;
use crate::StorageProcessor;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum L2TxSubmissionResult {
    Added,
    Replaced,
    AlreadyExecuted,
    Duplicate,
}
impl fmt::Display for L2TxSubmissionResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct TransactionsDal<'c, 'a> {
    pub storage: &'c mut StorageProcessor<'a>,
}

type TxLocations = Vec<(MiniblockNumber, Vec<(H256, u32, u16)>)>;

impl TransactionsDal<'_, '_> {
    pub fn insert_transaction_l1(&mut self, tx: L1Tx, l1_block_number: L1BlockNumber) {
        async_std::task::block_on(async {
            let contract_address = tx.execute.contract_address.as_bytes().to_vec();
            let tx_hash = tx.hash().0.to_vec();
            let json_data = serde_json::to_value(&tx.execute)
                .unwrap_or_else(|_| panic!("cannot serialize tx {:?} to json", tx.hash()));
            let gas_limit = u256_to_big_decimal(tx.common_data.gas_limit);
            let full_fee = u256_to_big_decimal(tx.common_data.full_fee);
            let layer_2_tip_fee = u256_to_big_decimal(tx.common_data.layer_2_tip_fee);
            let sender = tx.common_data.sender.0.to_vec();
            let serial_id = tx.serial_id().0 as i64;
            let gas_per_pubdata_limit = u256_to_big_decimal(tx.common_data.gas_per_pubdata_limit);
            let value = u256_to_big_decimal(tx.execute.value);
            let tx_format = tx.common_data.tx_format() as i32;

            let to_mint = u256_to_big_decimal(tx.common_data.to_mint);
            let refund_recipient = tx.common_data.refund_recipient.as_bytes().to_vec();

            let secs = (tx.received_timestamp_ms / 1000) as i64;
            let nanosecs = ((tx.received_timestamp_ms % 1000) * 1_000_000) as u32;
            let received_at = NaiveDateTime::from_timestamp_opt(secs, nanosecs).unwrap();

            sqlx::query!(
                "
                INSERT INTO transactions
                (
                    hash,
                    is_priority,
                    initiator_address,

                    gas_limit,
                    gas_per_pubdata_limit,

                    data,
                    priority_op_id,
                    full_fee,
                    layer_2_tip_fee,
                    contract_address,
                    l1_block_number,
                    value,

                    paymaster,
                    paymaster_input,
                    tx_format,

                    l1_tx_mint,
                    l1_tx_refund_recipient,

                    received_at,
                    created_at,
                    updated_at
                )
                VALUES
                    (
                        $1, TRUE, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                        $13, $14, $15, $16, $17, now(), now()
                    )
                ",
                tx_hash,
                sender,
                gas_limit,
                gas_per_pubdata_limit,
                json_data,
                serial_id,
                full_fee,
                layer_2_tip_fee,
                contract_address,
                l1_block_number.0 as i32,
                value,
                &Address::default().0.to_vec(),
                &vec![],
                tx_format,
                to_mint,
                refund_recipient,
                received_at,
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn insert_transaction_l2(
        &mut self,
        tx: L2Tx,
        exec_info: TransactionExecutionMetrics,
    ) -> L2TxSubmissionResult {
        async_std::task::block_on(async {
            let contract_address = tx.execute.contract_address.as_bytes().to_vec();
            let tx_hash = tx.hash().0.to_vec();
            let json_data = serde_json::to_value(&tx.execute)
                .unwrap_or_else(|_| panic!("cannot serialize tx {:?} to json", tx.hash()));
            let gas_limit = u256_to_big_decimal(tx.common_data.fee.gas_limit);
            let max_fee_per_gas = u256_to_big_decimal(tx.common_data.fee.max_fee_per_gas);
            let max_priority_fee_per_gas =
                u256_to_big_decimal(tx.common_data.fee.max_priority_fee_per_gas);
            let gas_per_pubdata_limit =
                u256_to_big_decimal(tx.common_data.fee.gas_per_pubdata_limit);
            let tx_format = tx.common_data.transaction_type as i32;
            let initiator = tx.initiator_account().0.to_vec();
            let signature = tx.common_data.signature.clone();
            let nonce = tx.common_data.nonce.0 as i64;
            let input_data = tx
                .common_data
                .input
                .clone()
                .expect("Data is mandatory")
                .data;
            let value = u256_to_big_decimal(tx.execute.value);
            let paymaster = tx.common_data.paymaster_params.paymaster.0.to_vec();
            let paymaster_input = tx.common_data.paymaster_params.paymaster_input.clone();
            let secs = (tx.received_timestamp_ms / 1000) as i64;
            let nanosecs = ((tx.received_timestamp_ms % 1000) * 1_000_000) as u32;
            let received_at = NaiveDateTime::from_timestamp_opt(secs, nanosecs).unwrap();
            // Besides just adding or updating(on conflict) the record, we want to extract some info
            // from the query below, to indicate what actually happened:
            // 1) transaction is added
            // 2) transaction is replaced
            // 3) WHERE clause conditions for DO UPDATE block were not met, so the transaction can't be replaced
            // the subquery in RETURNING clause looks into pre-UPDATE state of the table. So if the subquery will return NULL
            // transaction is fresh and was added to db(the second condition of RETURNING clause checks it).
            // Otherwise, if the subquery won't return NULL it means that there is already tx with such nonce and initiator_address in DB
            // and we can replace it WHERE clause conditions are met.
            // It is worth mentioning that if WHERE clause conditions are not met, None will be returned.
            let query_result = sqlx::query!(
                r#"
                INSERT INTO transactions
                (
                    hash,
                    is_priority,
                    initiator_address,
                    nonce,
                    signature,
                    gas_limit,
                    max_fee_per_gas,
                    max_priority_fee_per_gas,
                    gas_per_pubdata_limit,
                    input,
                    data,
                    tx_format,
                    contract_address,
                    value,
                    paymaster,
                    paymaster_input,
                    execution_info,
                    received_at,
                    created_at,
                    updated_at
                )
                VALUES
                    (
                        $1, FALSE, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                        jsonb_build_object('gas_used', $16::bigint, 'storage_writes', $17::int, 'contracts_used', $18::int),
                        $19, now(), now()
                    )
                ON CONFLICT
                    (initiator_address, nonce)
                DO UPDATE
                    SET hash=$1,
                        signature=$4,
                        gas_limit=$5,
                        max_fee_per_gas=$6,
                        max_priority_fee_per_gas=$7,
                        gas_per_pubdata_limit=$8,
                        input=$9,
                        data=$10,
                        tx_format=$11,
                        contract_address=$12,
                        value=$13,
                        paymaster=$14,
                        paymaster_input=$15,
                        execution_info=jsonb_build_object('gas_used', $16::bigint, 'storage_writes', $17::int, 'contracts_used', $18::int),
                        in_mempool=FALSE,
                        received_at=$19,
                        created_at=now(),
                        updated_at=now(),
                        error = NULL
                    WHERE transactions.is_priority = FALSE AND transactions.miniblock_number IS NULL
                    RETURNING (SELECT hash FROM transactions WHERE transactions.initiator_address = $2 AND transactions.nonce = $3) IS NOT NULL as "is_replaced!"
                "#,
                &tx_hash,
                &initiator,
                nonce,
                &signature,
                gas_limit,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                gas_per_pubdata_limit,
                input_data,
                &json_data,
                tx_format,
                contract_address,
                value,
                &paymaster,
                &paymaster_input,
                exec_info.gas_used as i64,
                (exec_info.initial_storage_writes + exec_info.repeated_storage_writes) as i32,
                exec_info.contracts_used as i32,
                received_at
            )
                .fetch_optional(self.storage.conn())
                .await
                .map(|option_record| option_record.map(|record| record.is_replaced));

            let l2_tx_insertion_result = match query_result {
                Ok(option_query_result) => match option_query_result {
                    Some(true) => L2TxSubmissionResult::Replaced,
                    Some(false) => L2TxSubmissionResult::Added,
                    None => L2TxSubmissionResult::AlreadyExecuted,
                },
                Err(err) => {
                    // So, we consider a tx hash to be a primary key of the transaction
                    // Based on the idea that we can't have two transactions with the same hash
                    // We assume that if there already exists some transaction with some tx hash
                    // another tx with the same tx hash is supposed to have the same data
                    // In this case we identify it as Duplicate
                    // Note, this error can happen because of the race condition (tx can be taken by several
                    // api servers, that simultaneously start execute it and try to inserted to DB)
                    if let error::Error::Database(ref error) = err {
                        if let Some(constraint) = error.constraint() {
                            if constraint == "transactions_pkey" {
                                return L2TxSubmissionResult::Duplicate;
                            }
                        }
                    }
                    panic!("{}", err);
                }
            };
            vlog::debug!(
                "{:?} l2 transaction {:?} to DB. init_acc {:?} nonce {:?} returned option {:?}",
                l2_tx_insertion_result,
                tx.hash(),
                tx.initiator_account(),
                tx.nonce(),
                l2_tx_insertion_result
            );

            l2_tx_insertion_result
        })
    }

    pub fn mark_txs_as_executed_in_l1_batch(
        &mut self,
        block_number: L1BatchNumber,
        transactions: &[TransactionExecutionResult],
    ) {
        async_std::task::block_on(async {
            let hashes: Vec<Vec<u8>> = transactions
                .iter()
                .map(|tx| tx.hash.as_bytes().to_vec())
                .collect();
            let l1_batch_tx_indexes = Vec::from_iter(0..transactions.len() as i32);
            sqlx::query!(
                "
                    UPDATE transactions
                    SET 
                        l1_batch_number = $3,
                        l1_batch_tx_index = data_table.l1_batch_tx_index,
                        updated_at = now()
                    FROM
                        (SELECT
                                UNNEST($1::int[]) AS l1_batch_tx_index,
                                UNNEST($2::bytea[]) AS hash
                        ) AS data_table
                    WHERE transactions.hash=data_table.hash 
                ",
                &l1_batch_tx_indexes,
                &hashes,
                block_number.0 as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn set_correct_tx_type_for_priority_operations(&mut self, limit: u32) -> bool {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                UPDATE transactions 
                SET tx_format=255 
                WHERE hash IN (
                    SELECT hash 
                    FROM transactions
                    WHERE is_priority = true
                      AND tx_format is null
                    LIMIT $1
                )
                RETURNING tx_format
                "#,
                limit as i32
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .is_some()
        })
    }

    pub fn mark_txs_as_executed_in_miniblock(
        &mut self,
        miniblock_number: MiniblockNumber,
        transactions: &[TransactionExecutionResult],
        block_base_fee_per_gas: U256,
    ) {
        async_std::task::block_on(async {
            let mut l1_hashes = Vec::with_capacity(transactions.len());
            let mut l1_indices_in_block = Vec::with_capacity(transactions.len());
            let mut l1_errors = Vec::with_capacity(transactions.len());
            let mut l1_execution_infos = Vec::with_capacity(transactions.len());

            let mut l2_hashes = Vec::with_capacity(transactions.len());
            let mut l2_indices_in_block = Vec::with_capacity(transactions.len());
            let mut l2_initiators = Vec::with_capacity(transactions.len());
            let mut l2_nonces = Vec::with_capacity(transactions.len());
            let mut l2_signatures = Vec::with_capacity(transactions.len());
            let mut l2_tx_formats = Vec::with_capacity(transactions.len());
            let mut l2_errors = Vec::with_capacity(transactions.len());
            let mut l2_effective_gas_prices = Vec::with_capacity(transactions.len());
            let mut l2_execution_infos = Vec::with_capacity(transactions.len());
            let mut l2_inputs = Vec::with_capacity(transactions.len());
            let mut l2_datas = Vec::with_capacity(transactions.len());
            let mut l2_gas_limits = Vec::with_capacity(transactions.len());
            let mut l2_max_fees_per_gas = Vec::with_capacity(transactions.len());
            let mut l2_max_priority_fees_per_gas = Vec::with_capacity(transactions.len());
            let mut l2_gas_per_pubdata_limit = Vec::with_capacity(transactions.len());
            let mut l2_refunded_gas = Vec::with_capacity(transactions.len());

            transactions
                .iter()
                .enumerate()
                .for_each(|(index_in_block, tx_res)| {
                    let TransactionExecutionResult {
                        hash,
                        execution_info,
                        transaction,
                        execution_status,
                        refunded_gas,
                        ..
                    } = tx_res;

                    // Bootloader currently doesn't return detailed errors.
                    let error = match execution_status {
                        TxExecutionStatus::Success => None,
                        // The string error used here is copied from the previous version.
                        // It is applied to every failed transaction -
                        // currently detailed errors are not supported.
                        TxExecutionStatus::Failure => Some("Bootloader-based tx failed".to_owned()),
                    };

                    match &transaction.common_data {
                        ExecuteTransactionCommon::L1(_) => {
                            l1_hashes.push(hash.0.to_vec());
                            l1_indices_in_block.push(index_in_block as i32);
                            l1_errors.push(error.unwrap_or_default());
                            l1_execution_infos.push(serde_json::to_value(execution_info).unwrap());
                        }
                        ExecuteTransactionCommon::L2(common_data) => {
                            let data = serde_json::to_value(&transaction.execute).unwrap();
                            l2_hashes.push(hash.0.to_vec());
                            l2_indices_in_block.push(index_in_block as i32);
                            l2_initiators.push(transaction.initiator_account().0.to_vec());
                            l2_nonces.push(common_data.nonce.0 as i32);
                            l2_signatures.push(common_data.signature.clone());
                            l2_tx_formats.push(common_data.transaction_type as i32);
                            l2_errors.push(error.unwrap_or_default());
                            let l2_effective_gas_price = common_data
                                .fee
                                .get_effective_gas_price(block_base_fee_per_gas);
                            l2_effective_gas_prices
                                .push(u256_to_big_decimal(l2_effective_gas_price));
                            l2_execution_infos.push(serde_json::to_value(execution_info).unwrap());
                            // Normally input data is mandatory
                            l2_inputs.push(common_data.input_data().unwrap_or_default());
                            l2_datas.push(data);
                            l2_gas_limits.push(u256_to_big_decimal(common_data.fee.gas_limit));
                            l2_max_fees_per_gas
                                .push(u256_to_big_decimal(common_data.fee.max_fee_per_gas));
                            l2_max_priority_fees_per_gas.push(u256_to_big_decimal(
                                common_data.fee.max_priority_fee_per_gas,
                            ));
                            l2_gas_per_pubdata_limit
                                .push(u256_to_big_decimal(common_data.fee.gas_per_pubdata_limit));
                            l2_refunded_gas.push(*refunded_gas as i64);
                        }
                    }
                });

            if !l2_hashes.is_empty() {
                // Update l2 txs

                // Due to the current tx replacement model, it's possible that tx has been replaced,
                // but the original was executed in memory,
                // so we have to update all fields for tx from fields stored in memory.
                sqlx::query!(
                    r#"
                        UPDATE transactions
                            SET 
                                hash = data_table.hash,
                                signature = data_table.signature,
                                gas_limit = data_table.gas_limit,
                                max_fee_per_gas = data_table.max_fee_per_gas,
                                max_priority_fee_per_gas = data_table.max_priority_fee_per_gas,
                                gas_per_pubdata_limit = data_table.gas_per_pubdata_limit,
                                input = data_table.input,
                                data = data_table.data,
                                tx_format = data_table.tx_format,
                                miniblock_number = $17,
                                index_in_block = data_table.index_in_block,
                                error = NULLIF(data_table.error, ''),
                                effective_gas_price = data_table.effective_gas_price,
                                execution_info = data_table.new_execution_info,
                                refunded_gas = data_table.refunded_gas,
                                in_mempool = FALSE,
                                updated_at = now()
                        FROM
                            (
                                SELECT
                                    UNNEST($1::bytea[]) AS initiator_address,
                                    UNNEST($2::int[]) AS nonce,
                                    UNNEST($3::bytea[]) AS hash,
                                    UNNEST($4::bytea[]) AS signature,
                                    UNNEST($5::numeric[]) AS gas_limit,
                                    UNNEST($6::numeric[]) AS max_fee_per_gas,
                                    UNNEST($7::numeric[]) AS max_priority_fee_per_gas,
                                    UNNEST($8::numeric[]) AS gas_per_pubdata_limit,
                                    UNNEST($9::int[]) AS tx_format,
                                    UNNEST($10::integer[]) AS index_in_block,
                                    UNNEST($11::varchar[]) AS error,
                                    UNNEST($12::numeric[]) AS effective_gas_price,
                                    UNNEST($13::jsonb[]) AS new_execution_info,
                                    UNNEST($14::bytea[]) AS input,
                                    UNNEST($15::jsonb[]) AS data,
                                    UNNEST($16::bigint[]) as refunded_gas
                            ) AS data_table
                        WHERE transactions.initiator_address=data_table.initiator_address 
                        AND transactions.nonce=data_table.nonce
                    "#,
                    &l2_initiators,
                    &l2_nonces,
                    &l2_hashes,
                    &l2_signatures,
                    &l2_gas_limits,
                    &l2_max_fees_per_gas,
                    &l2_max_priority_fees_per_gas,
                    &l2_gas_per_pubdata_limit,
                    &l2_tx_formats,
                    &l2_indices_in_block,
                    &l2_errors,
                    &l2_effective_gas_prices,
                    &l2_execution_infos,
                    &l2_inputs,
                    &l2_datas,
                    &l2_refunded_gas,
                    miniblock_number.0 as i32,
                )
                .execute(self.storage.conn())
                .await
                .unwrap();
            }

            // We can't replace l1 transaction, so we simply write the execution result
            if !l1_hashes.is_empty() {
                sqlx::query!(
                    r#"
                        UPDATE transactions
                            SET
                                miniblock_number = $1,
                                index_in_block = data_table.index_in_block,
                                error = NULLIF(data_table.error, ''),
                                in_mempool=FALSE,
                                execution_info = execution_info || data_table.new_execution_info,
                                updated_at = now()
                        FROM
                            (
                                SELECT
                                    UNNEST($2::bytea[]) AS hash,
                                    UNNEST($3::integer[]) AS index_in_block,
                                    UNNEST($4::varchar[]) AS error,
                                    UNNEST($5::jsonb[]) AS new_execution_info
                            ) AS data_table
                        WHERE transactions.hash = data_table.hash
                    "#,
                    miniblock_number.0 as i32,
                    &l1_hashes,
                    &l1_indices_in_block,
                    &l1_errors,
                    &l1_execution_infos
                )
                .execute(self.storage.conn())
                .await
                .unwrap();
            }
        })
    }

    pub fn mark_tx_as_rejected(&mut self, transaction_hash: H256, error: &str) {
        async_std::task::block_on(async {
            // If the rejected tx has been replaced, it means that this tx hash does not exist in the database
            // and we will update nothing.
            // These txs don't affect the state, so we can just easily skip this update.
            sqlx::query!(
                "UPDATE transactions
                    SET error = $1, updated_at = now()
                    WHERE hash = $2",
                error,
                transaction_hash.0.to_vec()
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn reset_transactions_state(&mut self, miniblock_number: MiniblockNumber) {
        async_std::task::block_on(async {
            sqlx::query!(
                "UPDATE transactions
                    SET l1_batch_number = NULL, miniblock_number = NULL, error = NULL, index_in_block = NULL, execution_info = '{}'
                    WHERE miniblock_number > $1",
                miniblock_number.0 as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn remove_stuck_txs(&mut self, stuck_tx_timeout: Duration) -> usize {
        async_std::task::block_on(async {
            let stuck_tx_timeout = pg_interval_from_duration(stuck_tx_timeout);
            sqlx::query!(
                "DELETE FROM transactions \
                 WHERE miniblock_number IS NULL AND received_at < now() - $1::interval \
                 AND is_priority=false AND error IS NULL \
                 RETURNING hash",
                stuck_tx_timeout
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .len()
        })
    }

    /// Fetches new updates for mempool
    /// Returns new transactions and current nonces for related accounts
    /// Latter is only used to bootstrap mempool for given account
    pub fn sync_mempool(
        &mut self,
        stashed_accounts: Vec<Address>,
        purged_accounts: Vec<Address>,
        gas_per_pubdata: u32,
        fee_per_gas: u64,
        limit: usize,
    ) -> (Vec<Transaction>, HashMap<Address, Nonce>) {
        async_std::task::block_on(async {
            let stashed_addresses: Vec<_> =
                stashed_accounts.into_iter().map(|a| a.0.to_vec()).collect();
            sqlx::query!(
                "UPDATE transactions SET in_mempool = FALSE \
                FROM UNNEST ($1::bytea[]) AS s(address) \
                WHERE transactions.in_mempool = TRUE AND transactions.initiator_address = s.address",
                &stashed_addresses,
            )
            .execute(self.storage.conn())
            .await
            .unwrap();

            let purged_addresses: Vec<_> =
                purged_accounts.into_iter().map(|a| a.0.to_vec()).collect();
            sqlx::query!(
                "DELETE FROM transactions \
                WHERE in_mempool = TRUE AND initiator_address = ANY($1)",
                &purged_addresses[..]
            )
            .execute(self.storage.conn())
            .await
            .unwrap();

            let transactions = sqlx::query_as!(
                StorageTransaction,
                "UPDATE transactions
                SET in_mempool = TRUE
                FROM (
                    SELECT hash
                    FROM transactions
                    WHERE miniblock_number IS NULL AND in_mempool = FALSE AND error IS NULL
                        AND (is_priority = TRUE OR (max_fee_per_gas >= $2 and gas_per_pubdata_limit >= $3))
                    ORDER BY is_priority DESC, priority_op_id, received_at
                    LIMIT $1
                ) as subquery
                WHERE transactions.hash = subquery.hash
                RETURNING transactions.*",
                limit as i32,
                BigDecimal::from(fee_per_gas),
                BigDecimal::from(gas_per_pubdata),
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();

            let nonce_keys: HashMap<_, _> = transactions
                .iter()
                .map(|tx| {
                    let address = Address::from_slice(&tx.initiator_address);
                    let nonce_key = get_nonce_key(&address).hashed_key();
                    (nonce_key, address)
                })
                .collect();

            let storage_keys: Vec<_> = nonce_keys.keys().map(|key| key.0.to_vec()).collect();
            let nonces: HashMap<_, _> = sqlx::query!(
                r#"SELECT hashed_key, value as "value!" FROM storage WHERE hashed_key = ANY($1)"#,
                &storage_keys,
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| {
                let nonce_key = H256::from_slice(&row.hashed_key);
                let nonce = Nonce(h256_to_u32(H256::from_slice(&row.value)));

                (*nonce_keys.get(&nonce_key).unwrap(), nonce)
            })
            .collect();

            (
                transactions.into_iter().map(|tx| tx.into()).collect(),
                nonces,
            )
        })
    }

    pub fn reset_mempool(&mut self) {
        async_std::task::block_on(async {
            sqlx::query!("UPDATE transactions SET in_mempool = FALSE WHERE in_mempool = TRUE")
                .execute(self.storage.conn())
                .await
                .unwrap();
        })
    }

    pub fn get_last_processed_l1_block(&mut self) -> Option<L1BlockNumber> {
        async_std::task::block_on(async {
            sqlx::query!(
                "SELECT l1_block_number FROM transactions
                WHERE priority_op_id IS NOT NULL
                ORDER BY priority_op_id DESC
                LIMIT 1"
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .and_then(|x| x.l1_block_number.map(|block| L1BlockNumber(block as u32)))
        })
    }

    pub fn last_priority_id(&mut self) -> Option<PriorityOpId> {
        async_std::task::block_on(async {
            let op_id = sqlx::query!(
                r#"SELECT MAX(priority_op_id) as "op_id" from transactions where is_priority = true"#
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()?
            .op_id?;
            Some(PriorityOpId(op_id as u64))
        })
    }

    pub fn next_priority_id(&mut self) -> PriorityOpId {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"SELECT MAX(priority_op_id) as "op_id" from transactions where is_priority = true AND miniblock_number IS NOT NULL"#
            )
                .fetch_optional(self.storage.conn())
                .await
                .unwrap()
                .and_then(|row| row.op_id)
                .map(|value| PriorityOpId((value + 1) as u64))
                .unwrap_or_default()
        })
    }

    pub fn insert_trace(&mut self, hash: H256, trace: VmExecutionTrace) {
        async_std::task::block_on(async {
            sqlx::query!(
                "INSERT INTO transaction_traces (tx_hash, trace, created_at, updated_at) VALUES ($1, $2, now(), now())",
                hash.as_bytes(),
                serde_json::to_value(trace).unwrap()
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn get_trace(&mut self, hash: H256) -> Option<VmExecutionTrace> {
        async_std::task::block_on(async {
            let trace = sqlx::query!(
                "SELECT trace FROM transaction_traces WHERE tx_hash = $1",
                hash.as_bytes()
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .map(|record| record.trace);
            trace.map(|trace| {
                serde_json::from_value(trace)
                    .unwrap_or_else(|_| panic!("invalid trace json in database for {:?}", hash))
            })
        })
    }

    // Returns transactions that state_keeper needs to reexecute on restart.
    // That is the transactions that are included to some miniblock,
    // but not included to L1 batch. The order of the transactions is the same as it was
    // during the previous execution.
    pub fn get_transactions_to_reexecute(&mut self) -> Vec<(MiniblockNumber, Vec<Transaction>)> {
        async_std::task::block_on(async {
            sqlx::query_as!(
                StorageTransaction,
                "
                    SELECT * FROM transactions
                    WHERE miniblock_number IS NOT NULL AND l1_batch_number IS NULL
                    ORDER BY miniblock_number, index_in_block
                ",
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .group_by(|tx| tx.miniblock_number)
            .into_iter()
            .map(|(miniblock_number, txs)| {
                (
                    MiniblockNumber(miniblock_number.unwrap() as u32),
                    txs.map(Into::<Transaction>::into)
                        .collect::<Vec<Transaction>>(),
                )
            })
            .collect()
        })
    }

    pub fn get_tx_locations(&mut self, l1_batch_number: L1BatchNumber) -> TxLocations {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                    SELECT miniblock_number as "miniblock_number!",
                        hash, index_in_block as "index_in_block!", l1_batch_tx_index as "l1_batch_tx_index!"
                    FROM transactions
                    WHERE l1_batch_number = $1
                    ORDER BY miniblock_number, index_in_block
                "#,
                l1_batch_number.0 as i64
            )
                .fetch_all(self.storage.conn())
                .await
                .unwrap()
                .into_iter()
                .group_by(|tx| tx.miniblock_number)
                .into_iter()
                .map(|(miniblock_number, rows)| {
                    (
                        MiniblockNumber(miniblock_number as u32),
                        rows.map(|row| (H256::from_slice(&row.hash), row.index_in_block as u32, row.l1_batch_tx_index as u16))
                            .collect::<Vec<(H256, u32, u16)>>(),
                    )
                })
                .collect()
        })
    }
}
