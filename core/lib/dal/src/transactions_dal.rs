use std::{cmp::min, collections::HashMap, fmt, time::Duration};

use bigdecimal::BigDecimal;
use itertools::Itertools;
use sqlx::types::chrono::NaiveDateTime;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
    utils::pg_interval_from_duration,
};
use zksync_types::{
    block::L2BlockExecutionData, debug_flat_call::CallTraceMeta, l1::L1Tx, l2::L2Tx,
    protocol_upgrade::ProtocolUpgradeTx, Address, ExecuteTransactionCommon, L1BatchNumber,
    L1BlockNumber, L2BlockNumber, PriorityOpId, ProtocolVersionId, Transaction,
    TransactionTimeRangeConstraint, H256, PROTOCOL_UPGRADE_TX_TYPE, U256,
};
use zksync_vm_interface::{
    tracer::ValidationTraces, Call, TransactionExecutionMetrics, TransactionExecutionResult,
    TxExecutionStatus,
};

use crate::{
    models::{
        storage_transaction::{parse_call_trace, serialize_call_into_bytes, StorageTransaction},
        u256_to_big_decimal,
    },
    Core, CoreDal,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum L2TxSubmissionResult {
    Added,
    Replaced,
    AlreadyExecuted,
    Duplicate,
    Proxied,
    InsertionInProgress,
}

impl fmt::Display for L2TxSubmissionResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Added => "added",
            Self::Replaced => "replaced",
            Self::AlreadyExecuted => "already_executed",
            Self::Duplicate => "duplicate",
            Self::Proxied => "proxied",
            Self::InsertionInProgress => "insertion_in_progress",
        })
    }
}

#[derive(Debug)]
pub struct TransactionsDal<'c, 'a> {
    pub(crate) storage: &'c mut Connection<'a, Core>,
}

/// In transaction insertion methods, we intentionally override `tx.received_timestamp_ms` to have well-defined causal ordering
/// among transactions (transactions persisted earlier should have earlier timestamp). Causal ordering by the received timestamp
/// is assumed in some logic dealing with transactions, e.g., pending transaction filters on the API server.
impl TransactionsDal<'_, '_> {
    pub async fn insert_transaction_l1(
        &mut self,
        tx: &L1Tx,
        l1_block_number: L1BlockNumber,
    ) -> DalResult<()> {
        let contract_address = tx.execute.contract_address;
        let contract_address_as_bytes = contract_address.map(|addr| addr.as_bytes().to_vec());
        let tx_hash = tx.hash();
        let tx_hash_bytes = tx_hash.as_bytes();
        let json_data = serde_json::to_value(&tx.execute)
            .unwrap_or_else(|_| panic!("cannot serialize tx {:?} to json", tx.hash()));
        let gas_limit = u256_to_big_decimal(tx.common_data.gas_limit);
        let max_fee_per_gas = u256_to_big_decimal(tx.common_data.max_fee_per_gas);
        let full_fee = u256_to_big_decimal(tx.common_data.full_fee);
        let layer_2_tip_fee = u256_to_big_decimal(tx.common_data.layer_2_tip_fee);
        let sender = tx.common_data.sender.as_bytes();
        let serial_id = tx.serial_id().0 as i64;
        let gas_per_pubdata_limit = u256_to_big_decimal(tx.common_data.gas_per_pubdata_limit);
        let value = u256_to_big_decimal(tx.execute.value);
        let tx_format = tx.common_data.tx_format() as i32;
        let empty_address = Address::default();

        let to_mint = u256_to_big_decimal(tx.common_data.to_mint);
        let refund_recipient = tx.common_data.refund_recipient.as_bytes();

        sqlx::query!(
            r#"
            INSERT INTO
            transactions (
                hash,
                is_priority,
                initiator_address,
                gas_limit,
                max_fee_per_gas,
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
                $1,
                TRUE,
                $2,
                $3,
                $4,
                $5,
                $6,
                $7,
                $8,
                $9,
                $10,
                $11,
                $12,
                $13,
                $14,
                $15,
                $16,
                $17,
                NOW(),
                NOW(),
                NOW()
            )
            ON CONFLICT (hash) DO NOTHING
            "#,
            tx_hash_bytes,
            sender,
            gas_limit,
            max_fee_per_gas,
            gas_per_pubdata_limit,
            json_data,
            serial_id,
            full_fee,
            layer_2_tip_fee,
            contract_address_as_bytes,
            l1_block_number.0 as i32,
            value,
            empty_address.as_bytes(),
            &[] as &[u8],
            tx_format,
            to_mint,
            refund_recipient,
        )
        .instrument("insert_transaction_l1")
        .with_arg("tx_hash", &tx_hash)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_l1_transactions_hashes(&mut self, start_id: usize) -> DalResult<Vec<H256>> {
        let hashes = sqlx::query!(
            r#"
            SELECT
                hash
            FROM
                transactions
            WHERE
                priority_op_id >= $1
                AND is_priority = TRUE
            ORDER BY
                priority_op_id
            "#,
            start_id as i64
        )
        .instrument("get_l1_transactions_hashes")
        .with_arg("start_id", &start_id)
        .fetch_all(self.storage)
        .await?;
        Ok(hashes
            .into_iter()
            .map(|row| H256::from_slice(&row.hash))
            .collect())
    }

    pub async fn insert_system_transaction(&mut self, tx: &ProtocolUpgradeTx) -> DalResult<()> {
        let contract_address = tx.execute.contract_address;
        let contract_address_as_bytes = contract_address.map(|addr| addr.as_bytes().to_vec());
        let tx_hash = tx.common_data.hash().0.to_vec();
        let json_data = serde_json::to_value(&tx.execute)
            .unwrap_or_else(|_| panic!("cannot serialize tx {:?} to json", tx.common_data.hash()));
        let upgrade_id = tx.common_data.upgrade_id as i32;
        let gas_limit = u256_to_big_decimal(tx.common_data.gas_limit);
        let max_fee_per_gas = u256_to_big_decimal(tx.common_data.max_fee_per_gas);
        let sender = tx.common_data.sender.0.to_vec();
        let gas_per_pubdata_limit = u256_to_big_decimal(tx.common_data.gas_per_pubdata_limit);
        let value = u256_to_big_decimal(tx.execute.value);
        let tx_format = tx.common_data.tx_format() as i32;
        let l1_block_number = tx.common_data.eth_block as i32;

        let to_mint = u256_to_big_decimal(tx.common_data.to_mint);
        let refund_recipient = tx.common_data.refund_recipient.as_bytes().to_vec();

        sqlx::query!(
            r#"
            INSERT INTO
            transactions (
                hash,
                is_priority,
                initiator_address,
                gas_limit,
                max_fee_per_gas,
                gas_per_pubdata_limit,
                data,
                upgrade_id,
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
                $1,
                TRUE,
                $2,
                $3,
                $4,
                $5,
                $6,
                $7,
                $8,
                $9,
                $10,
                $11,
                $12,
                $13,
                $14,
                $15,
                NOW(),
                NOW(),
                NOW()
            )
            ON CONFLICT (hash) DO NOTHING
            "#,
            tx_hash,
            sender,
            gas_limit,
            max_fee_per_gas,
            gas_per_pubdata_limit,
            json_data,
            upgrade_id,
            contract_address_as_bytes,
            l1_block_number,
            value,
            &Address::default().0.to_vec(),
            &vec![],
            tx_format,
            to_mint,
            refund_recipient,
        )
        .instrument("insert_system_transaction")
        .with_arg("tx_hash", &tx_hash)
        .fetch_optional(self.storage)
        .await?;
        Ok(())
    }

    pub async fn insert_transaction_l2(
        &mut self,
        tx: &L2Tx,
        exec_info: TransactionExecutionMetrics,
        validation_traces: ValidationTraces,
    ) -> DalResult<L2TxSubmissionResult> {
        let tx_hash = tx.hash();
        let is_duplicate = sqlx::query!(
            r#"
            SELECT
                TRUE
            FROM
                transactions
            WHERE
                hash = $1
            "#,
            tx_hash.as_bytes(),
        )
        .instrument("insert_transaction_l2#is_duplicate")
        .with_arg("tx_hash", &tx_hash)
        .fetch_optional(self.storage)
        .await?
        .is_some();

        if is_duplicate {
            tracing::debug!("Prevented inserting duplicate L2 transaction {tx_hash:?} to DB");
            return Ok(L2TxSubmissionResult::Duplicate);
        }

        let initiator_address = tx.initiator_account();
        let contract_address = tx.execute.contract_address;
        let contract_address_as_bytes = contract_address.map(|addr| addr.as_bytes().to_vec());
        let json_data = serde_json::to_value(&tx.execute)
            .unwrap_or_else(|_| panic!("cannot serialize tx {:?} to json", tx.hash()));
        let gas_limit = u256_to_big_decimal(tx.common_data.fee.gas_limit);
        let max_fee_per_gas = u256_to_big_decimal(tx.common_data.fee.max_fee_per_gas);
        let max_priority_fee_per_gas =
            u256_to_big_decimal(tx.common_data.fee.max_priority_fee_per_gas);
        let gas_per_pubdata_limit = u256_to_big_decimal(tx.common_data.fee.gas_per_pubdata_limit);
        let tx_format = tx.common_data.transaction_type as i32;
        let signature = &tx.common_data.signature;
        let nonce = i64::from(tx.common_data.nonce.0);
        let input_data = &tx
            .common_data
            .input
            .as_ref()
            .expect("Data is mandatory")
            .data;
        let value = u256_to_big_decimal(tx.execute.value);
        let paymaster = tx.common_data.paymaster_params.paymaster.0.as_ref();
        let paymaster_input = &tx.common_data.paymaster_params.paymaster_input;

        let max_timestamp = NaiveDateTime::MAX.and_utc().timestamp() as u64;
        #[allow(deprecated)]
        let timestamp_asserter_range_start =
            validation_traces.timestamp_asserter_range.clone().map(|x| {
                NaiveDateTime::from_timestamp_opt(min(x.start, max_timestamp) as i64, 0).unwrap()
            });
        #[allow(deprecated)]
        let timestamp_asserter_range_end = validation_traces.timestamp_asserter_range.map(|x| {
            NaiveDateTime::from_timestamp_opt(min(x.end, max_timestamp) as i64, 0).unwrap()
        });
        // Besides just adding or updating(on conflict) the record, we want to extract some info
        // from the query below, to indicate what actually happened:
        // 1) transaction is added
        // 2) transaction is replaced
        // 3) WHERE clause conditions for DO UPDATE block were not met, so the transaction can't be replaced
        // the subquery in RETURNING clause looks into pre-UPDATE state of the table. So if the subquery will return NULL
        // transaction is fresh and was added to db(the second condition of RETURNING clause checks it).
        // Otherwise, if the subquery won't return NULL it means that there is already tx with such nonce and `initiator_address` in DB
        // and we can replace it WHERE clause conditions are met.
        // It is worth mentioning that if WHERE clause conditions are not met, None will be returned.
        let query_result = sqlx::query!(
            r#"
            INSERT INTO
            transactions (
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
                timestamp_asserter_range_start,
                timestamp_asserter_range_end,
                created_at,
                updated_at
            )
            VALUES
            (
                $1,
                FALSE,
                $2,
                $3,
                $4,
                $5,
                $6,
                $7,
                $8,
                $9,
                $10,
                $11,
                $12,
                $13,
                $14,
                $15,
                JSONB_BUILD_OBJECT(
                    'gas_used',
                    $16::BIGINT,
                    'storage_writes',
                    $17::INT,
                    'contracts_used',
                    $18::INT
                ),
                NOW(),
                $19,
                $20,
                NOW(),
                NOW()
            )
            ON CONFLICT (initiator_address, nonce) DO
            UPDATE
            SET
            hash = $1,
            signature = $4,
            gas_limit = $5,
            max_fee_per_gas = $6,
            max_priority_fee_per_gas = $7,
            gas_per_pubdata_limit = $8,
            input = $9,
            data = $10,
            tx_format = $11,
            contract_address = $12,
            value = $13,
            paymaster = $14,
            paymaster_input = $15,
            execution_info
            = JSONB_BUILD_OBJECT(
                'gas_used',
                $16::BIGINT,
                'storage_writes',
                $17::INT,
                'contracts_used',
                $18::INT
            ),
            in_mempool = FALSE,
            received_at = NOW(),
            timestamp_asserter_range_start = $19,
            timestamp_asserter_range_end = $20,
            created_at = NOW(),
            updated_at = NOW(),
            error = NULL
            WHERE
            transactions.is_priority = FALSE
            AND transactions.miniblock_number IS NULL
            RETURNING
            (
                SELECT
                    hash
                FROM
                    transactions
                WHERE
                    transactions.initiator_address = $2
                    AND transactions.nonce = $3
            ) IS NOT NULL AS "is_replaced!"
            "#,
            tx_hash.as_bytes(),
            initiator_address.as_bytes(),
            nonce,
            &signature,
            gas_limit,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            gas_per_pubdata_limit,
            input_data,
            &json_data,
            tx_format,
            contract_address_as_bytes,
            value,
            &paymaster,
            &paymaster_input,
            exec_info.vm.gas_used as i64,
            (exec_info.writes.initial_storage_writes + exec_info.writes.repeated_storage_writes)
                as i32,
            exec_info.vm.contracts_used as i32,
            timestamp_asserter_range_start,
            timestamp_asserter_range_end,
        )
        .instrument("insert_transaction_l2")
        .with_arg("tx_hash", &tx_hash)
        .fetch_optional(self.storage)
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
                // API servers, that simultaneously start execute it and try to inserted to DB)
                if let sqlx::Error::Database(error) = err.inner() {
                    if let Some(constraint) = error.constraint() {
                        if constraint == "transactions_pkey" {
                            tracing::debug!(
                                "Attempted to insert duplicate L2 transaction {tx_hash:?} to DB"
                            );
                            return Ok(L2TxSubmissionResult::Duplicate);
                        }
                    }
                }
                return Err(err);
            }
        };
        tracing::debug!(
            "{:?} l2 transaction {:?} to DB. init_acc {:?} nonce {:?} returned option {:?}",
            l2_tx_insertion_result,
            tx_hash,
            initiator_address,
            nonce,
            l2_tx_insertion_result
        );

        Ok(l2_tx_insertion_result)
    }

    pub async fn mark_txs_as_executed_in_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
        tx_hashes: &[H256],
    ) -> DalResult<()> {
        let hashes: Vec<_> = tx_hashes.iter().map(H256::as_bytes).collect();
        let l1_batch_tx_indexes: Vec<_> = (0..tx_hashes.len() as i32).collect();
        sqlx::query!(
            r#"
            UPDATE transactions
            SET
                l1_batch_number = $3,
                l1_batch_tx_index = data_table.l1_batch_tx_index,
                updated_at = NOW()
            FROM
                (
                    SELECT
                        UNNEST($1::INT []) AS l1_batch_tx_index,
                        UNNEST($2::BYTEA []) AS hash
                ) AS data_table
            WHERE
                transactions.hash = data_table.hash
            "#,
            &l1_batch_tx_indexes,
            &hashes as &[&[u8]],
            i64::from(l1_batch_number.0)
        )
        .instrument("mark_txs_as_executed_in_l1_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .with_arg("transactions.len", &tx_hashes.len())
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn mark_txs_as_executed_in_l2_block(
        &mut self,
        l2_block_number: L2BlockNumber,
        transactions: &[TransactionExecutionResult],
        block_base_fee_per_gas: U256,
        protocol_version: ProtocolVersionId,
        // On the main node, transactions are inserted into the DB by API servers.
        // However on the EN, they need to be inserted after they are executed by the state keeper.
        insert_txs: bool,
    ) -> DalResult<()> {
        let mut transaction = self.storage.start_transaction().await?;

        let mut call_traces_tx_hashes = Vec::with_capacity(transactions.len());
        let mut bytea_call_traces = Vec::with_capacity(transactions.len());
        for tx_res in transactions {
            if let Some(call_trace) = tx_res.call_trace() {
                bytea_call_traces.push(serialize_call_into_bytes(call_trace, protocol_version));
                call_traces_tx_hashes.push(tx_res.hash.as_bytes());
            }
        }

        if insert_txs {
            // There can be transactions in the DB in case of block rollback or if the DB was restored from a dump.
            let tx_hashes: Vec<_> = transactions.iter().map(|tx| tx.hash.as_bytes()).collect();
            sqlx::query!(
                r#"
                DELETE FROM transactions
                WHERE
                    hash = ANY($1)
                "#,
                &tx_hashes as &[&[u8]],
            )
            .instrument("mark_txs_as_executed_in_l2_block#remove_old_txs_with_hashes")
            .execute(&mut transaction)
            .await?;
            for tx in transactions {
                let Some(nonce) = tx.transaction.nonce() else {
                    // Nonce is missing for L1 and upgrade txs, they can be skipped in this loop.
                    continue;
                };
                let initiator = tx.transaction.initiator_account();
                sqlx::query!(
                    r#"
                    DELETE FROM transactions
                    WHERE
                        initiator_address = $1
                        AND nonce = $2
                    "#,
                    initiator.as_bytes(),
                    nonce.0 as i32,
                )
                .instrument("mark_txs_as_executed_in_l2_block#remove_old_txs_with_addr_and_nonce")
                .execute(&mut transaction)
                .await?;
            }

            // Different transaction types have different sets of fields to insert so we handle them separately.
            transaction
                .transactions_dal()
                .insert_executed_l2_transactions(
                    l2_block_number,
                    block_base_fee_per_gas,
                    transactions,
                )
                .await?;
            transaction
                .transactions_dal()
                .insert_executed_l1_transactions(l2_block_number, transactions)
                .await?;
            transaction
                .transactions_dal()
                .insert_executed_upgrade_transactions(l2_block_number, transactions)
                .await?;
        } else {
            // Different transaction types have different sets of fields to update so we handle them separately.
            transaction
                .transactions_dal()
                .update_executed_l2_transactions(
                    l2_block_number,
                    block_base_fee_per_gas,
                    transactions,
                )
                .await?;
            transaction
                .transactions_dal()
                .update_executed_l1_transactions(l2_block_number, transactions)
                .await?;
            transaction
                .transactions_dal()
                .update_executed_upgrade_transactions(l2_block_number, transactions)
                .await?;
        }

        if !bytea_call_traces.is_empty() {
            sqlx::query!(
                r#"
                INSERT INTO
                call_traces (tx_hash, call_trace)
                SELECT
                    u.tx_hash,
                    u.call_trace
                FROM
                    UNNEST($1::bytea [], $2::bytea []) AS u (tx_hash, call_trace)
                "#,
                &call_traces_tx_hashes as &[&[u8]],
                &bytea_call_traces
            )
            .instrument("insert_call_tracer")
            .report_latency()
            .execute(&mut transaction)
            .await?;
        }

        transaction.commit().await
    }

    // Bootloader currently doesn't return detailed errors.
    fn map_transaction_error(tx_res: &TransactionExecutionResult) -> &'static str {
        match &tx_res.execution_status {
            TxExecutionStatus::Success => "",
            // The string error used here is copied from the previous version.
            // It is applied to every failed transaction -
            // currently detailed errors are not supported.
            TxExecutionStatus::Failure => "Bootloader-based tx failed",
        }
    }

    async fn insert_executed_l2_transactions(
        &mut self,
        l2_block_number: L2BlockNumber,
        block_base_fee_per_gas: U256,
        transactions: &[TransactionExecutionResult],
    ) -> DalResult<()> {
        let l2_txs_len = transactions
            .iter()
            .filter(|tx_res| {
                matches!(
                    tx_res.transaction.common_data,
                    ExecuteTransactionCommon::L2(_)
                )
            })
            .count();
        if l2_txs_len == 0 {
            return Ok(());
        }

        let instrumentation = Instrumented::new("mark_txs_as_executed_in_l2_block#insert_l2_txs")
            .with_arg("l2_block_number", &l2_block_number)
            .with_arg("l2_txs.len", &l2_txs_len);

        let mut l2_hashes = Vec::with_capacity(l2_txs_len);
        let mut l2_initiators = Vec::with_capacity(l2_txs_len);
        let mut l2_nonces = Vec::with_capacity(l2_txs_len);
        let mut l2_signatures = Vec::with_capacity(l2_txs_len);
        let mut l2_gas_limits = Vec::with_capacity(l2_txs_len);
        let mut l2_max_fees_per_gas = Vec::with_capacity(l2_txs_len);
        let mut l2_max_priority_fees_per_gas = Vec::with_capacity(l2_txs_len);
        let mut l2_gas_per_pubdata_limit = Vec::with_capacity(l2_txs_len);
        let mut l2_inputs = Vec::with_capacity(l2_txs_len);
        let mut l2_datas = Vec::with_capacity(l2_txs_len);
        let mut l2_tx_formats = Vec::with_capacity(l2_txs_len);
        let mut l2_contract_addresses = Vec::with_capacity(l2_txs_len);
        let mut l2_values = Vec::with_capacity(l2_txs_len);
        let mut l2_paymaster = Vec::with_capacity(l2_txs_len);
        let mut l2_paymaster_input = Vec::with_capacity(l2_txs_len);
        let mut l2_execution_infos = Vec::with_capacity(l2_txs_len);
        let mut l2_indices_in_block = Vec::with_capacity(l2_txs_len);
        let mut l2_errors = Vec::with_capacity(l2_txs_len);
        let mut l2_effective_gas_prices = Vec::with_capacity(l2_txs_len);
        let mut l2_refunded_gas = Vec::with_capacity(l2_txs_len);

        for (index_in_block, tx_res) in transactions.iter().enumerate() {
            let transaction = &tx_res.transaction;
            let ExecuteTransactionCommon::L2(common_data) = &transaction.common_data else {
                continue;
            };

            let data = serde_json::to_value(&transaction.execute).map_err(|err| {
                instrumentation.arg_error(
                    &format!("transactions[{index_in_block}].transaction.execute"),
                    err,
                )
            })?;
            let l2_execution_info = serde_json::to_value(tx_res.execution_info).map_err(|err| {
                instrumentation.arg_error(
                    &format!("transactions[{index_in_block}].execution_info"),
                    err,
                )
            })?;
            let refunded_gas = i64::try_from(tx_res.refunded_gas).map_err(|err| {
                instrumentation
                    .arg_error(&format!("transactions[{index_in_block}].refunded_gas"), err)
            })?;

            let contract_address = transaction.execute.contract_address;
            let contract_address_as_bytes = contract_address.map(|addr| addr.as_bytes().to_vec());
            l2_values.push(u256_to_big_decimal(transaction.execute.value));
            l2_contract_addresses.push(contract_address_as_bytes);
            l2_paymaster_input.push(&common_data.paymaster_params.paymaster_input[..]);
            l2_paymaster.push(common_data.paymaster_params.paymaster.as_bytes());
            l2_hashes.push(tx_res.hash.as_bytes());
            l2_indices_in_block.push(index_in_block as i32);
            l2_initiators.push(transaction.initiator_account().0);
            l2_nonces.push(common_data.nonce.0 as i32);
            l2_signatures.push(&common_data.signature[..]);
            l2_tx_formats.push(common_data.transaction_type as i32);
            l2_errors.push(Self::map_transaction_error(tx_res));
            let l2_effective_gas_price = common_data
                .fee
                .get_effective_gas_price(block_base_fee_per_gas);
            l2_effective_gas_prices.push(u256_to_big_decimal(l2_effective_gas_price));
            l2_execution_infos.push(l2_execution_info);
            // Normally input data is mandatory
            l2_inputs.push(common_data.input_data().unwrap_or_default());
            l2_datas.push(data);
            l2_gas_limits.push(u256_to_big_decimal(common_data.fee.gas_limit));
            l2_max_fees_per_gas.push(u256_to_big_decimal(common_data.fee.max_fee_per_gas));
            l2_max_priority_fees_per_gas.push(u256_to_big_decimal(
                common_data.fee.max_priority_fee_per_gas,
            ));
            l2_gas_per_pubdata_limit
                .push(u256_to_big_decimal(common_data.fee.gas_per_pubdata_limit));
            l2_refunded_gas.push(refunded_gas);
        }

        let query = sqlx::query!(
            r#"
            INSERT INTO
            transactions (
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
                miniblock_number,
                index_in_block,
                error,
                effective_gas_price,
                refunded_gas,
                received_at,
                created_at,
                updated_at
            )
            SELECT
                data_table.hash,
                FALSE,
                data_table.initiator_address,
                data_table.nonce,
                data_table.signature,
                data_table.gas_limit,
                data_table.max_fee_per_gas,
                data_table.max_priority_fee_per_gas,
                data_table.gas_per_pubdata_limit,
                data_table.input,
                data_table.data,
                data_table.tx_format,
                data_table.contract_address,
                data_table.value,
                data_table.paymaster,
                data_table.paymaster_input,
                data_table.new_execution_info,
                $21,
                data_table.index_in_block,
                NULLIF(data_table.error, ''),
                data_table.effective_gas_price,
                data_table.refunded_gas,
                NOW(),
                NOW(),
                NOW()
            FROM
                (
                    SELECT
                        UNNEST($1::bytea []) AS hash,
                        UNNEST($2::bytea []) AS initiator_address,
                        UNNEST($3::int []) AS nonce,
                        UNNEST($4::bytea []) AS signature,
                        UNNEST($5::numeric []) AS gas_limit,
                        UNNEST($6::numeric []) AS max_fee_per_gas,
                        UNNEST($7::numeric []) AS max_priority_fee_per_gas,
                        UNNEST($8::numeric []) AS gas_per_pubdata_limit,
                        UNNEST($9::bytea []) AS input,
                        UNNEST($10::jsonb []) AS data,
                        UNNEST($11::int []) AS tx_format,
                        UNNEST($12::bytea []) AS contract_address,
                        UNNEST($13::numeric []) AS value,
                        UNNEST($14::bytea []) AS paymaster,
                        UNNEST($15::bytea []) AS paymaster_input,
                        UNNEST($16::jsonb []) AS new_execution_info,
                        UNNEST($17::integer []) AS index_in_block,
                        UNNEST($18::varchar []) AS error,
                        UNNEST($19::numeric []) AS effective_gas_price,
                        UNNEST($20::bigint []) AS refunded_gas
                ) AS data_table
            "#,
            &l2_hashes as &[&[u8]],
            &l2_initiators as &[[u8; 20]],
            &l2_nonces,
            &l2_signatures as &[&[u8]],
            &l2_gas_limits,
            &l2_max_fees_per_gas,
            &l2_max_priority_fees_per_gas,
            &l2_gas_per_pubdata_limit,
            &l2_inputs as &[&[u8]],
            &l2_datas,
            &l2_tx_formats,
            &l2_contract_addresses as &[Option<Vec<u8>>],
            &l2_values,
            &l2_paymaster as &[&[u8]],
            &l2_paymaster_input as &[&[u8]],
            &l2_execution_infos,
            &l2_indices_in_block,
            &l2_errors as &[&str],
            &l2_effective_gas_prices,
            &l2_refunded_gas,
            l2_block_number.0 as i32,
        );

        instrumentation.with(query).execute(self.storage).await?;
        Ok(())
    }

    async fn update_executed_l2_transactions(
        &mut self,
        l2_block_number: L2BlockNumber,
        block_base_fee_per_gas: U256,
        transactions: &[TransactionExecutionResult],
    ) -> DalResult<()> {
        let l2_txs_len = transactions
            .iter()
            .filter(|tx_res| {
                matches!(
                    tx_res.transaction.common_data,
                    ExecuteTransactionCommon::L2(_)
                )
            })
            .count();
        if l2_txs_len == 0 {
            return Ok(());
        }

        let instrumentation = Instrumented::new("mark_txs_as_executed_in_l2_block#update_l2_txs")
            .with_arg("l2_block_number", &l2_block_number)
            .with_arg("l2_txs.len", &l2_txs_len);

        let mut l2_hashes = Vec::with_capacity(l2_txs_len);
        let mut l2_values = Vec::with_capacity(l2_txs_len);
        let mut l2_contract_addresses = Vec::with_capacity(l2_txs_len);
        let mut l2_paymaster = Vec::with_capacity(l2_txs_len);
        let mut l2_paymaster_input = Vec::with_capacity(l2_txs_len);
        let mut l2_indices_in_block = Vec::with_capacity(l2_txs_len);
        let mut l2_initiators = Vec::with_capacity(l2_txs_len);
        let mut l2_nonces = Vec::with_capacity(l2_txs_len);
        let mut l2_signatures = Vec::with_capacity(l2_txs_len);
        let mut l2_tx_formats = Vec::with_capacity(l2_txs_len);
        let mut l2_errors = Vec::with_capacity(l2_txs_len);
        let mut l2_effective_gas_prices = Vec::with_capacity(l2_txs_len);
        let mut l2_execution_infos = Vec::with_capacity(l2_txs_len);
        let mut l2_inputs = Vec::with_capacity(l2_txs_len);
        let mut l2_datas = Vec::with_capacity(l2_txs_len);
        let mut l2_gas_limits = Vec::with_capacity(l2_txs_len);
        let mut l2_max_fees_per_gas = Vec::with_capacity(l2_txs_len);
        let mut l2_max_priority_fees_per_gas = Vec::with_capacity(l2_txs_len);
        let mut l2_gas_per_pubdata_limit = Vec::with_capacity(l2_txs_len);
        let mut l2_refunded_gas = Vec::with_capacity(l2_txs_len);

        for (index_in_block, tx_res) in transactions.iter().enumerate() {
            let transaction = &tx_res.transaction;
            let ExecuteTransactionCommon::L2(common_data) = &transaction.common_data else {
                continue;
            };

            let data = serde_json::to_value(&transaction.execute).map_err(|err| {
                instrumentation.arg_error(
                    &format!("transactions[{index_in_block}].transaction.execute"),
                    err,
                )
            })?;
            let l2_execution_info = serde_json::to_value(tx_res.execution_info).map_err(|err| {
                instrumentation.arg_error(
                    &format!("transactions[{index_in_block}].execution_info"),
                    err,
                )
            })?;
            let refunded_gas = i64::try_from(tx_res.refunded_gas).map_err(|err| {
                instrumentation
                    .arg_error(&format!("transactions[{index_in_block}].refunded_gas"), err)
            })?;

            let contract_address = transaction.execute.contract_address;
            let contract_address_as_bytes = contract_address.map(|addr| addr.as_bytes().to_vec());
            l2_values.push(u256_to_big_decimal(transaction.execute.value));
            l2_contract_addresses.push(contract_address_as_bytes);
            l2_paymaster_input.push(&common_data.paymaster_params.paymaster_input[..]);
            l2_paymaster.push(common_data.paymaster_params.paymaster.as_bytes());
            l2_hashes.push(tx_res.hash.as_bytes());
            l2_indices_in_block.push(index_in_block as i32);
            l2_initiators.push(transaction.initiator_account().0);
            l2_nonces.push(common_data.nonce.0 as i32);
            l2_signatures.push(&common_data.signature[..]);
            l2_tx_formats.push(common_data.transaction_type as i32);
            l2_errors.push(Self::map_transaction_error(tx_res));
            let l2_effective_gas_price = common_data
                .fee
                .get_effective_gas_price(block_base_fee_per_gas);
            l2_effective_gas_prices.push(u256_to_big_decimal(l2_effective_gas_price));
            l2_execution_infos.push(l2_execution_info);
            // Normally input data is mandatory
            l2_inputs.push(common_data.input_data().unwrap_or_default());
            l2_datas.push(data);
            l2_gas_limits.push(u256_to_big_decimal(common_data.fee.gas_limit));
            l2_max_fees_per_gas.push(u256_to_big_decimal(common_data.fee.max_fee_per_gas));
            l2_max_priority_fees_per_gas.push(u256_to_big_decimal(
                common_data.fee.max_priority_fee_per_gas,
            ));
            l2_gas_per_pubdata_limit
                .push(u256_to_big_decimal(common_data.fee.gas_per_pubdata_limit));
            l2_refunded_gas.push(refunded_gas);
        }

        // Due to the current tx replacement model, it's possible that tx has been replaced,
        // but the original was executed in memory,
        // so we have to update all fields for tx from fields stored in memory.
        // Note, that transactions are updated in order of their hashes to avoid deadlocks with other UPDATE queries.
        let query = sqlx::query!(
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
                miniblock_number = $21,
                index_in_block = data_table.index_in_block,
                error = NULLIF(data_table.error, ''),
                effective_gas_price = data_table.effective_gas_price,
                execution_info = data_table.new_execution_info,
                refunded_gas = data_table.refunded_gas,
                value = data_table.value,
                contract_address = data_table.contract_address,
                paymaster = data_table.paymaster,
                paymaster_input = data_table.paymaster_input,
                in_mempool = FALSE,
                updated_at = NOW()
            FROM
                (
                    SELECT
                        data_table_temp.*
                    FROM
                        (
                            SELECT
                                UNNEST($1::bytea []) AS initiator_address,
                                UNNEST($2::int []) AS nonce,
                                UNNEST($3::bytea []) AS hash,
                                UNNEST($4::bytea []) AS signature,
                                UNNEST($5::numeric []) AS gas_limit,
                                UNNEST($6::numeric []) AS max_fee_per_gas,
                                UNNEST($7::numeric []) AS max_priority_fee_per_gas,
                                UNNEST($8::numeric []) AS gas_per_pubdata_limit,
                                UNNEST($9::int []) AS tx_format,
                                UNNEST($10::integer []) AS index_in_block,
                                UNNEST($11::varchar []) AS error,
                                UNNEST($12::numeric []) AS effective_gas_price,
                                UNNEST($13::jsonb []) AS new_execution_info,
                                UNNEST($14::bytea []) AS input,
                                UNNEST($15::jsonb []) AS data,
                                UNNEST($16::bigint []) AS refunded_gas,
                                UNNEST($17::numeric []) AS value,
                                UNNEST($18::bytea []) AS contract_address,
                                UNNEST($19::bytea []) AS paymaster,
                                UNNEST($20::bytea []) AS paymaster_input
                        ) AS data_table_temp
                    JOIN transactions
                        ON
                            transactions.initiator_address
                            = data_table_temp.initiator_address
                            AND transactions.nonce = data_table_temp.nonce
                    ORDER BY
                        transactions.hash
                ) AS data_table
            WHERE
                transactions.initiator_address = data_table.initiator_address
                AND transactions.nonce = data_table.nonce
            "#,
            &l2_initiators as &[[u8; 20]],
            &l2_nonces,
            &l2_hashes as &[&[u8]],
            &l2_signatures as &[&[u8]],
            &l2_gas_limits,
            &l2_max_fees_per_gas,
            &l2_max_priority_fees_per_gas,
            &l2_gas_per_pubdata_limit,
            &l2_tx_formats,
            &l2_indices_in_block,
            &l2_errors as &[&str],
            &l2_effective_gas_prices,
            &l2_execution_infos,
            &l2_inputs as &[&[u8]],
            &l2_datas,
            &l2_refunded_gas,
            &l2_values,
            &l2_contract_addresses as &[Option<Vec<u8>>],
            &l2_paymaster as &[&[u8]],
            &l2_paymaster_input as &[&[u8]],
            l2_block_number.0 as i32,
        );

        instrumentation.with(query).execute(self.storage).await?;
        Ok(())
    }

    async fn insert_executed_l1_transactions(
        &mut self,
        l2_block_number: L2BlockNumber,
        transactions: &[TransactionExecutionResult],
    ) -> DalResult<()> {
        let l1_txs_len = transactions
            .iter()
            .filter(|tx_res| {
                matches!(
                    tx_res.transaction.common_data,
                    ExecuteTransactionCommon::L1(_)
                )
            })
            .count();
        if l1_txs_len == 0 {
            return Ok(());
        }

        let instrumentation = Instrumented::new("mark_txs_as_executed_in_l2_block#insert_l1_txs")
            .with_arg("l2_block_number", &l2_block_number)
            .with_arg("l1_txs.len", &l1_txs_len);

        let mut l1_hashes = Vec::with_capacity(l1_txs_len);
        let mut l1_initiator_address = Vec::with_capacity(l1_txs_len);
        let mut l1_gas_limit = Vec::with_capacity(l1_txs_len);
        let mut l1_max_fee_per_gas = Vec::with_capacity(l1_txs_len);
        let mut l1_gas_per_pubdata_limit = Vec::with_capacity(l1_txs_len);
        let mut l1_data = Vec::with_capacity(l1_txs_len);
        let mut l1_priority_op_id = Vec::with_capacity(l1_txs_len);
        let mut l1_full_fee = Vec::with_capacity(l1_txs_len);
        let mut l1_layer_2_tip_fee = Vec::with_capacity(l1_txs_len);
        let mut l1_contract_address = Vec::with_capacity(l1_txs_len);
        let mut l1_l1_block_number = Vec::with_capacity(l1_txs_len);
        let mut l1_value = Vec::with_capacity(l1_txs_len);
        let mut l1_tx_format = Vec::with_capacity(l1_txs_len);
        let mut l1_tx_mint = Vec::with_capacity(l1_txs_len);
        let mut l1_tx_refund_recipient = Vec::with_capacity(l1_txs_len);
        let mut l1_indices_in_block = Vec::with_capacity(l1_txs_len);
        let mut l1_errors = Vec::with_capacity(l1_txs_len);
        let mut l1_execution_infos = Vec::with_capacity(l1_txs_len);
        let mut l1_refunded_gas = Vec::with_capacity(l1_txs_len);
        let mut l1_effective_gas_prices = Vec::with_capacity(l1_txs_len);

        for (index_in_block, tx_res) in transactions.iter().enumerate() {
            let transaction = &tx_res.transaction;
            let ExecuteTransactionCommon::L1(common_data) = &transaction.common_data else {
                continue;
            };

            let l1_execution_info = serde_json::to_value(tx_res.execution_info).map_err(|err| {
                instrumentation.arg_error(
                    &format!("transactions[{index_in_block}].execution_info"),
                    err,
                )
            })?;
            let refunded_gas = i64::try_from(tx_res.refunded_gas).map_err(|err| {
                instrumentation
                    .arg_error(&format!("transactions[{index_in_block}].refunded_gas"), err)
            })?;

            let contract_address = transaction.execute.contract_address;
            let contract_address_as_bytes = contract_address.map(|addr| addr.as_bytes().to_vec());
            let tx = &tx_res.transaction;
            l1_hashes.push(tx_res.hash.as_bytes());
            l1_initiator_address.push(common_data.sender.as_bytes());
            l1_gas_limit.push(u256_to_big_decimal(common_data.gas_limit));
            l1_max_fee_per_gas.push(u256_to_big_decimal(common_data.max_fee_per_gas));
            l1_gas_per_pubdata_limit.push(u256_to_big_decimal(common_data.gas_per_pubdata_limit));
            l1_data.push(
                serde_json::to_value(&tx.execute)
                    .unwrap_or_else(|_| panic!("cannot serialize tx {:?} to json", tx.hash())),
            );
            l1_priority_op_id.push(common_data.serial_id.0 as i64);
            l1_full_fee.push(u256_to_big_decimal(common_data.full_fee));
            l1_layer_2_tip_fee.push(u256_to_big_decimal(common_data.layer_2_tip_fee));
            l1_contract_address.push(contract_address_as_bytes);
            l1_l1_block_number.push(common_data.eth_block as i32);
            l1_value.push(u256_to_big_decimal(tx.execute.value));
            l1_tx_format.push(common_data.tx_format() as i32);
            l1_tx_mint.push(u256_to_big_decimal(common_data.to_mint));
            l1_tx_refund_recipient.push(common_data.refund_recipient.as_bytes());
            l1_indices_in_block.push(index_in_block as i32);
            l1_errors.push(Self::map_transaction_error(tx_res));
            l1_execution_infos.push(l1_execution_info);
            l1_refunded_gas.push(refunded_gas);
            l1_effective_gas_prices.push(u256_to_big_decimal(common_data.max_fee_per_gas));
        }

        let query = sqlx::query!(
            r#"
            INSERT INTO
            transactions (
                hash,
                is_priority,
                initiator_address,
                gas_limit,
                max_fee_per_gas,
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
                miniblock_number,
                index_in_block,
                error,
                execution_info,
                refunded_gas,
                effective_gas_price,
                received_at,
                created_at,
                updated_at
            )
            SELECT
                data_table.hash,
                TRUE,
                data_table.initiator_address,
                data_table.gas_limit,
                data_table.max_fee_per_gas,
                data_table.gas_per_pubdata_limit,
                data_table.data,
                data_table.priority_op_id,
                data_table.full_fee,
                data_table.layer_2_tip_fee,
                data_table.contract_address,
                data_table.l1_block_number,
                data_table.value,
                '\x0000000000000000000000000000000000000000'::bytea,
                '\x'::bytea,
                data_table.tx_format,
                data_table.l1_tx_mint,
                data_table.l1_tx_refund_recipient,
                $21,
                data_table.index_in_block,
                NULLIF(data_table.error, ''),
                data_table.execution_info,
                data_table.refunded_gas,
                data_table.effective_gas_price,
                NOW(),
                NOW(),
                NOW()
            FROM
                (
                    SELECT
                        UNNEST($1::bytea []) AS hash,
                        UNNEST($2::bytea []) AS initiator_address,
                        UNNEST($3::numeric []) AS gas_limit,
                        UNNEST($4::numeric []) AS max_fee_per_gas,
                        UNNEST($5::numeric []) AS gas_per_pubdata_limit,
                        UNNEST($6::jsonb []) AS data,
                        UNNEST($7::bigint []) AS priority_op_id,
                        UNNEST($8::numeric []) AS full_fee,
                        UNNEST($9::numeric []) AS layer_2_tip_fee,
                        UNNEST($10::bytea []) AS contract_address,
                        UNNEST($11::int []) AS l1_block_number,
                        UNNEST($12::numeric []) AS value,
                        UNNEST($13::integer []) AS tx_format,
                        UNNEST($14::numeric []) AS l1_tx_mint,
                        UNNEST($15::bytea []) AS l1_tx_refund_recipient,
                        UNNEST($16::int []) AS index_in_block,
                        UNNEST($17::varchar []) AS error,
                        UNNEST($18::jsonb []) AS execution_info,
                        UNNEST($19::bigint []) AS refunded_gas,
                        UNNEST($20::numeric []) AS effective_gas_price
                ) AS data_table
            "#,
            &l1_hashes as &[&[u8]],
            &l1_initiator_address as &[&[u8]],
            &l1_gas_limit,
            &l1_max_fee_per_gas,
            &l1_gas_per_pubdata_limit,
            &l1_data,
            &l1_priority_op_id,
            &l1_full_fee,
            &l1_layer_2_tip_fee,
            &l1_contract_address as &[Option<Vec<u8>>],
            &l1_l1_block_number,
            &l1_value,
            &l1_tx_format,
            &l1_tx_mint,
            &l1_tx_refund_recipient as &[&[u8]],
            &l1_indices_in_block,
            &l1_errors as &[&str],
            &l1_execution_infos,
            &l1_refunded_gas,
            &l1_effective_gas_prices,
            l2_block_number.0 as i32,
        );

        instrumentation.with(query).execute(self.storage).await?;
        Ok(())
    }

    async fn update_executed_l1_transactions(
        &mut self,
        l2_block_number: L2BlockNumber,
        transactions: &[TransactionExecutionResult],
    ) -> DalResult<()> {
        let l1_txs_len = transactions
            .iter()
            .filter(|tx_res| {
                matches!(
                    tx_res.transaction.common_data,
                    ExecuteTransactionCommon::L1(_)
                )
            })
            .count();
        if l1_txs_len == 0 {
            return Ok(());
        }

        let instrumentation = Instrumented::new("mark_txs_as_executed_in_l2_block#update_l1_txs")
            .with_arg("l2_block_number", &l2_block_number)
            .with_arg("l1_txs.len", &l1_txs_len);

        let mut l1_hashes = Vec::with_capacity(l1_txs_len);
        let mut l1_indices_in_block = Vec::with_capacity(l1_txs_len);
        let mut l1_errors = Vec::with_capacity(l1_txs_len);
        let mut l1_execution_infos = Vec::with_capacity(l1_txs_len);
        let mut l1_refunded_gas = Vec::with_capacity(l1_txs_len);
        let mut l1_effective_gas_prices = Vec::with_capacity(l1_txs_len);

        for (index_in_block, tx_res) in transactions.iter().enumerate() {
            let transaction = &tx_res.transaction;
            let ExecuteTransactionCommon::L1(common_data) = &transaction.common_data else {
                continue;
            };

            let l1_execution_info = serde_json::to_value(tx_res.execution_info).map_err(|err| {
                instrumentation.arg_error(
                    &format!("transactions[{index_in_block}].execution_info"),
                    err,
                )
            })?;
            let refunded_gas = i64::try_from(tx_res.refunded_gas).map_err(|err| {
                instrumentation
                    .arg_error(&format!("transactions[{index_in_block}].refunded_gas"), err)
            })?;

            l1_hashes.push(tx_res.hash.as_bytes());
            l1_indices_in_block.push(index_in_block as i32);
            l1_errors.push(Self::map_transaction_error(tx_res));
            l1_execution_infos.push(l1_execution_info);
            l1_refunded_gas.push(refunded_gas);
            l1_effective_gas_prices.push(u256_to_big_decimal(common_data.max_fee_per_gas));
        }

        let query = sqlx::query!(
            r#"
            UPDATE transactions
            SET
                miniblock_number = $1,
                index_in_block = data_table.index_in_block,
                error = NULLIF(data_table.error, ''),
                in_mempool = FALSE,
                execution_info = execution_info || data_table.new_execution_info,
                refunded_gas = data_table.refunded_gas,
                effective_gas_price = data_table.effective_gas_price,
                updated_at = NOW()
            FROM
                (
                    SELECT
                        UNNEST($2::bytea []) AS hash,
                        UNNEST($3::integer []) AS index_in_block,
                        UNNEST($4::varchar []) AS error,
                        UNNEST($5::jsonb []) AS new_execution_info,
                        UNNEST($6::bigint []) AS refunded_gas,
                        UNNEST($7::numeric []) AS effective_gas_price
                ) AS data_table
            WHERE
                transactions.hash = data_table.hash
            "#,
            l2_block_number.0 as i32,
            &l1_hashes as &[&[u8]],
            &l1_indices_in_block,
            &l1_errors as &[&str],
            &l1_execution_infos,
            &l1_refunded_gas,
            &l1_effective_gas_prices,
        );

        instrumentation.with(query).execute(self.storage).await?;
        Ok(())
    }

    async fn insert_executed_upgrade_transactions(
        &mut self,
        l2_block_number: L2BlockNumber,
        transactions: &[TransactionExecutionResult],
    ) -> DalResult<()> {
        let upgrade_txs_len = transactions
            .iter()
            .filter(|tx_res| {
                matches!(
                    tx_res.transaction.common_data,
                    ExecuteTransactionCommon::ProtocolUpgrade(_)
                )
            })
            .count();
        if upgrade_txs_len == 0 {
            return Ok(());
        }

        let instrumentation =
            Instrumented::new("mark_txs_as_executed_in_l2_block#insert_upgrade_txs")
                .with_arg("l2_block_number", &l2_block_number)
                .with_arg("upgrade_txs.len", &upgrade_txs_len);

        let mut upgrade_hashes = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_initiator_address = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_gas_limit = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_max_fee_per_gas = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_gas_per_pubdata_limit = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_data = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_upgrade_id = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_contract_address = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_l1_block_number = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_value = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_tx_format = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_tx_mint = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_tx_refund_recipient = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_indices_in_block = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_errors = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_execution_infos = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_refunded_gas = Vec::with_capacity(upgrade_txs_len);
        let mut upgrade_effective_gas_prices = Vec::with_capacity(upgrade_txs_len);

        for (index_in_block, tx_res) in transactions.iter().enumerate() {
            let transaction = &tx_res.transaction;
            let ExecuteTransactionCommon::ProtocolUpgrade(common_data) = &transaction.common_data
            else {
                continue;
            };

            let execution_info = serde_json::to_value(tx_res.execution_info).map_err(|err| {
                instrumentation.arg_error(
                    &format!("transactions[{index_in_block}].execution_info"),
                    err,
                )
            })?;
            let refunded_gas = i64::try_from(tx_res.refunded_gas).map_err(|err| {
                instrumentation
                    .arg_error(&format!("transactions[{index_in_block}].refunded_gas"), err)
            })?;

            let contract_address = transaction.execute.contract_address;
            let contract_address_as_bytes = contract_address.map(|addr| addr.as_bytes().to_vec());
            let tx = &tx_res.transaction;
            upgrade_hashes.push(tx_res.hash.as_bytes());
            upgrade_initiator_address.push(common_data.sender.as_bytes());
            upgrade_gas_limit.push(u256_to_big_decimal(common_data.gas_limit));
            upgrade_max_fee_per_gas.push(u256_to_big_decimal(common_data.max_fee_per_gas));
            upgrade_gas_per_pubdata_limit
                .push(u256_to_big_decimal(common_data.gas_per_pubdata_limit));
            upgrade_data.push(
                serde_json::to_value(&tx.execute)
                    .unwrap_or_else(|_| panic!("cannot serialize tx {:?} to json", tx.hash())),
            );
            upgrade_upgrade_id.push(common_data.upgrade_id as i32);
            upgrade_contract_address.push(contract_address_as_bytes);
            upgrade_l1_block_number.push(common_data.eth_block as i32);
            upgrade_value.push(u256_to_big_decimal(tx.execute.value));
            upgrade_tx_format.push(common_data.tx_format() as i32);
            upgrade_tx_mint.push(u256_to_big_decimal(common_data.to_mint));
            upgrade_tx_refund_recipient.push(common_data.refund_recipient.as_bytes());
            upgrade_indices_in_block.push(index_in_block as i32);
            upgrade_errors.push(Self::map_transaction_error(tx_res));
            upgrade_execution_infos.push(execution_info);
            upgrade_refunded_gas.push(refunded_gas);
            upgrade_effective_gas_prices.push(u256_to_big_decimal(common_data.max_fee_per_gas));
        }

        let query = sqlx::query!(
            r#"
            INSERT INTO
            transactions (
                hash,
                is_priority,
                initiator_address,
                gas_limit,
                max_fee_per_gas,
                gas_per_pubdata_limit,
                data,
                upgrade_id,
                contract_address,
                l1_block_number,
                value,
                paymaster,
                paymaster_input,
                tx_format,
                l1_tx_mint,
                l1_tx_refund_recipient,
                miniblock_number,
                index_in_block,
                error,
                execution_info,
                refunded_gas,
                effective_gas_price,
                received_at,
                created_at,
                updated_at
            )
            SELECT
                data_table.hash,
                TRUE,
                data_table.initiator_address,
                data_table.gas_limit,
                data_table.max_fee_per_gas,
                data_table.gas_per_pubdata_limit,
                data_table.data,
                data_table.upgrade_id,
                data_table.contract_address,
                data_table.l1_block_number,
                data_table.value,
                '\x0000000000000000000000000000000000000000'::bytea,
                '\x'::bytea,
                data_table.tx_format,
                data_table.l1_tx_mint,
                data_table.l1_tx_refund_recipient,
                $19,
                data_table.index_in_block,
                NULLIF(data_table.error, ''),
                data_table.execution_info,
                data_table.refunded_gas,
                data_table.effective_gas_price,
                NOW(),
                NOW(),
                NOW()
            FROM
                (
                    SELECT
                        UNNEST($1::bytea []) AS hash,
                        UNNEST($2::bytea []) AS initiator_address,
                        UNNEST($3::numeric []) AS gas_limit,
                        UNNEST($4::numeric []) AS max_fee_per_gas,
                        UNNEST($5::numeric []) AS gas_per_pubdata_limit,
                        UNNEST($6::jsonb []) AS data,
                        UNNEST($7::int []) AS upgrade_id,
                        UNNEST($8::bytea []) AS contract_address,
                        UNNEST($9::int []) AS l1_block_number,
                        UNNEST($10::numeric []) AS value,
                        UNNEST($11::integer []) AS tx_format,
                        UNNEST($12::numeric []) AS l1_tx_mint,
                        UNNEST($13::bytea []) AS l1_tx_refund_recipient,
                        UNNEST($14::int []) AS index_in_block,
                        UNNEST($15::varchar []) AS error,
                        UNNEST($16::jsonb []) AS execution_info,
                        UNNEST($17::bigint []) AS refunded_gas,
                        UNNEST($18::numeric []) AS effective_gas_price
                ) AS data_table
            "#,
            &upgrade_hashes as &[&[u8]],
            &upgrade_initiator_address as &[&[u8]],
            &upgrade_gas_limit,
            &upgrade_max_fee_per_gas,
            &upgrade_gas_per_pubdata_limit,
            &upgrade_data,
            &upgrade_upgrade_id,
            &upgrade_contract_address as &[Option<Vec<u8>>],
            &upgrade_l1_block_number,
            &upgrade_value,
            &upgrade_tx_format,
            &upgrade_tx_mint,
            &upgrade_tx_refund_recipient as &[&[u8]],
            &upgrade_indices_in_block,
            &upgrade_errors as &[&str],
            &upgrade_execution_infos,
            &upgrade_refunded_gas,
            &upgrade_effective_gas_prices,
            l2_block_number.0 as i32,
        );

        instrumentation.with(query).execute(self.storage).await?;
        Ok(())
    }

    async fn update_executed_upgrade_transactions(
        &mut self,
        l2_block_number: L2BlockNumber,
        transactions: &[TransactionExecutionResult],
    ) -> DalResult<()> {
        let upgrade_txs_len = transactions
            .iter()
            .filter(|tx_res| {
                matches!(
                    tx_res.transaction.common_data,
                    ExecuteTransactionCommon::ProtocolUpgrade(_)
                )
            })
            .count();
        if upgrade_txs_len == 0 {
            return Ok(());
        }

        let instrumentation =
            Instrumented::new("mark_txs_as_executed_in_l2_block#update_upgrade_txs")
                .with_arg("l2_block_number", &l2_block_number)
                .with_arg("upgrade_txs.len", &upgrade_txs_len);

        let mut upgrade_hashes = Vec::new();
        let mut upgrade_indices_in_block = Vec::new();
        let mut upgrade_errors = Vec::new();
        let mut upgrade_execution_infos = Vec::new();
        let mut upgrade_refunded_gas = Vec::new();
        let mut upgrade_effective_gas_prices = Vec::new();

        for (index_in_block, tx_res) in transactions.iter().enumerate() {
            let transaction = &tx_res.transaction;
            let ExecuteTransactionCommon::ProtocolUpgrade(common_data) = &transaction.common_data
            else {
                continue;
            };

            let execution_info = serde_json::to_value(tx_res.execution_info).map_err(|err| {
                instrumentation.arg_error(
                    &format!("transactions[{index_in_block}].execution_info"),
                    err,
                )
            })?;
            let refunded_gas = i64::try_from(tx_res.refunded_gas).map_err(|err| {
                instrumentation
                    .arg_error(&format!("transactions[{index_in_block}].refunded_gas"), err)
            })?;

            upgrade_hashes.push(tx_res.hash.as_bytes());
            upgrade_indices_in_block.push(index_in_block as i32);
            upgrade_errors.push(Self::map_transaction_error(tx_res));
            upgrade_execution_infos.push(execution_info);
            upgrade_refunded_gas.push(refunded_gas);
            upgrade_effective_gas_prices.push(u256_to_big_decimal(common_data.max_fee_per_gas));
        }

        let query = sqlx::query!(
            r#"
            UPDATE transactions
            SET
                miniblock_number = $1,
                index_in_block = data_table.index_in_block,
                error = NULLIF(data_table.error, ''),
                in_mempool = FALSE,
                execution_info = execution_info || data_table.new_execution_info,
                refunded_gas = data_table.refunded_gas,
                effective_gas_price = data_table.effective_gas_price,
                updated_at = NOW()
            FROM
                (
                    SELECT
                        UNNEST($2::bytea []) AS hash,
                        UNNEST($3::integer []) AS index_in_block,
                        UNNEST($4::varchar []) AS error,
                        UNNEST($5::jsonb []) AS new_execution_info,
                        UNNEST($6::bigint []) AS refunded_gas,
                        UNNEST($7::numeric []) AS effective_gas_price
                ) AS data_table
            WHERE
                transactions.hash = data_table.hash
            "#,
            l2_block_number.0 as i32,
            &upgrade_hashes as &[&[u8]],
            &upgrade_indices_in_block,
            &upgrade_errors as &[&str],
            &upgrade_execution_infos,
            &upgrade_refunded_gas,
            &upgrade_effective_gas_prices,
        );

        instrumentation.with(query).execute(self.storage).await?;
        Ok(())
    }

    pub async fn mark_tx_as_rejected(
        &mut self,
        transaction_hash: H256,
        error: &str,
    ) -> DalResult<()> {
        // If the rejected tx has been replaced, it means that this tx hash does not exist in the database
        // and we will update nothing.
        // These txs don't affect the state, so we can just easily skip this update.
        sqlx::query!(
            r#"
            UPDATE transactions
            SET
                error = $1,
                updated_at = NOW()
            WHERE
                hash = $2
            "#,
            error,
            transaction_hash.as_bytes()
        )
        .instrument("mark_tx_as_rejected")
        .with_arg("transaction_hash", &transaction_hash)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn reset_transactions_state(
        &mut self,
        l2_block_number: L2BlockNumber,
    ) -> DalResult<()> {
        let hash_rows = sqlx::query!(
            r#"
            UPDATE transactions
            SET
                l1_batch_number = NULL,
                miniblock_number = NULL,
                error = NULL,
                index_in_block = NULL,
                execution_info = '{}'
            WHERE
                miniblock_number > $1
            RETURNING
            hash
            "#,
            i64::from(l2_block_number.0)
        )
        .instrument("reset_transactions_state#get_tx_hashes")
        .with_arg("l2_block_number", &l2_block_number)
        .fetch_all(self.storage)
        .await?;

        let tx_hashes: Vec<_> = hash_rows.iter().map(|row| &row.hash[..]).collect();
        sqlx::query!(
            r#"
            DELETE FROM call_traces
            WHERE
                tx_hash = ANY($1)
            "#,
            &tx_hashes as &[&[u8]]
        )
        .instrument("reset_transactions_state")
        .with_arg("l2_block_number", &l2_block_number)
        .with_arg("tx_hashes.len", &tx_hashes.len())
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn remove_stuck_txs(&mut self, stuck_tx_timeout: Duration) -> DalResult<usize> {
        let stuck_tx_timeout = pg_interval_from_duration(stuck_tx_timeout);
        let rows = sqlx::query!(
            r#"
            DELETE FROM transactions
            WHERE
                miniblock_number IS NULL
                AND received_at < NOW() - $1::INTERVAL
                AND is_priority = FALSE
                AND error IS NULL
            RETURNING
            hash
            "#,
            stuck_tx_timeout
        )
        .instrument("remove_stuck_txs")
        .with_arg("stuck_tx_timeout", &stuck_tx_timeout)
        .fetch_all(self.storage)
        .await?;

        Ok(rows.len())
    }

    pub async fn get_priority_txs_in_mempool(&mut self) -> DalResult<usize> {
        let result = sqlx::query!(
            r#"
            SELECT COUNT(*) FROM transactions
            WHERE
                in_mempool = TRUE AND
                is_priority = TRUE
            "#
        )
        .instrument("get_l1_txs_in_mempool")
        .fetch_one(self.storage)
        .await?;

        Ok(result.count.unwrap_or_default() as usize)
    }

    /// Resets `in_mempool` to `FALSE` for the given transaction hashes.
    pub async fn reset_mempool_status(&mut self, transaction_hashes: &[H256]) -> DalResult<()> {
        // Convert H256 hashes into `&[u8]`
        let hashes: Vec<_> = transaction_hashes.iter().map(H256::as_bytes).collect();

        // Execute the UPDATE query
        let result = sqlx::query!(
            r#"
            UPDATE transactions
            SET
                in_mempool = FALSE
            WHERE
                hash = ANY($1)
            "#,
            &hashes as &[&[u8]]
        )
        .instrument("reset_mempool_status")
        .with_arg("transaction_hashes.len", &hashes.len())
        .execute(self.storage)
        .await?;

        // Log debug information about how many rows were affected
        tracing::debug!(
            "Updated {} transactions to in_mempool = false; provided hashes: {transaction_hashes:?}",
            result.rows_affected()
        );

        Ok(())
    }

    /// Fetches new updates for mempool. Returns new transactions and current nonces for related accounts;
    /// the latter are only used to bootstrap mempool for given account.
    pub async fn sync_mempool(
        &mut self,
        stashed_accounts: &[Address],
        purged_accounts: &[Address],
        gas_per_pubdata: u32,
        fee_per_gas: u64,
        allow_l1_txs: bool,
        limit: usize,
    ) -> DalResult<Vec<(Transaction, TransactionTimeRangeConstraint)>> {
        let stashed_addresses: Vec<_> = stashed_accounts.iter().map(Address::as_bytes).collect();
        let result = sqlx::query!(
            r#"
            UPDATE transactions
            SET
                in_mempool = FALSE
            FROM
                UNNEST($1::bytea []) AS s (address)
            WHERE
                transactions.in_mempool = TRUE
                AND transactions.initiator_address = s.address
            "#,
            &stashed_addresses as &[&[u8]],
        )
        .instrument("sync_mempool#update_stashed")
        .with_arg("stashed_addresses.len", &stashed_addresses.len())
        .execute(self.storage)
        .await?;

        tracing::debug!(
            "Updated {} transactions for stashed accounts, stashed accounts amount: {}, stashed_accounts: {:?}",
            result.rows_affected(),
            stashed_addresses.len(),
            stashed_accounts.iter().map(|a|format!("{:x}", a)).collect::<Vec<_>>()
        );

        let purged_addresses: Vec<_> = purged_accounts.iter().map(Address::as_bytes).collect();
        let result = sqlx::query!(
            r#"
            DELETE FROM transactions
            WHERE
                in_mempool = TRUE
                AND initiator_address = ANY($1)
            "#,
            &purged_addresses as &[&[u8]]
        )
        .instrument("sync_mempool#delete_purged")
        .with_arg("purged_addresses.len", &purged_addresses.len())
        .execute(self.storage)
        .await?;

        tracing::debug!(
            "Updated {} transactions for purged accounts, purged accounts amount: {}",
            result.rows_affected(),
            purged_addresses.len()
        );

        // Note, that transactions are updated in order of their hashes to avoid deadlocks with other UPDATE queries.
        let transactions = sqlx::query_as!(
            StorageTransaction,
            r#"
            UPDATE transactions
            SET
                in_mempool = TRUE
            FROM
                (
                    SELECT
                        hash
                    FROM
                        (
                            SELECT
                                hash
                            FROM
                                transactions
                            WHERE
                                miniblock_number IS NULL
                                AND in_mempool = FALSE
                                AND error IS NULL
                                AND (
                                    (
                                        is_priority = TRUE
                                        AND $5 = TRUE
                                    )
                                    OR (
                                        is_priority = FALSE
                                        AND max_fee_per_gas >= $2
                                        AND gas_per_pubdata_limit >= $3
                                    )
                                )
                                AND tx_format != $4
                            ORDER BY
                                is_priority DESC,
                                priority_op_id,
                                received_at
                            LIMIT
                                $1
                        ) AS subquery1
                    ORDER BY
                        hash
                ) AS subquery2
            WHERE
                transactions.hash = subquery2.hash
            RETURNING
            transactions.*
            "#,
            limit as i32,
            BigDecimal::from(fee_per_gas),
            BigDecimal::from(gas_per_pubdata),
            i32::from(PROTOCOL_UPGRADE_TX_TYPE),
            allow_l1_txs
        )
        .instrument("sync_mempool")
        .with_arg("fee_per_gas", &fee_per_gas)
        .with_arg("gas_per_pubdata", &gas_per_pubdata)
        .with_arg("limit", &limit)
        .with_arg("allow_l1_txs", &allow_l1_txs)
        .fetch_all(self.storage)
        .await?;

        let transactions_with_constraints = transactions
            .into_iter()
            .map(|tx| {
                let constraint = TransactionTimeRangeConstraint::from(&tx);
                (tx.into(), constraint)
            })
            .collect();
        Ok(transactions_with_constraints)
    }

    pub async fn reset_mempool(&mut self) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE transactions
            SET
                in_mempool = FALSE
            WHERE
                in_mempool = TRUE
            "#
        )
        .instrument("reset_mempool")
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn get_last_processed_l1_block(&mut self) -> DalResult<Option<L1BlockNumber>> {
        let maybe_row = sqlx::query!(
            r#"
            SELECT
                l1_block_number
            FROM
                transactions
            WHERE
                priority_op_id IS NOT NULL
            ORDER BY
                priority_op_id DESC
            LIMIT
                1
            "#
        )
        .instrument("get_last_processed_l1_block")
        .fetch_optional(self.storage)
        .await?;

        Ok(maybe_row
            .and_then(|row| row.l1_block_number)
            .map(|number| L1BlockNumber(number as u32)))
    }

    pub async fn last_priority_id(&mut self) -> DalResult<Option<PriorityOpId>> {
        let maybe_row = sqlx::query!(
            r#"
            SELECT
                MAX(priority_op_id) AS "op_id"
            FROM
                transactions
            WHERE
                is_priority = TRUE
            "#
        )
        .instrument("last_priority_id")
        .fetch_optional(self.storage)
        .await?;

        Ok(maybe_row
            .and_then(|row| row.op_id)
            .map(|op_id| PriorityOpId(op_id as u64)))
    }

    /// Returns the next ID after the ID of the last sealed priority operation.
    /// Doesn't work if node was recovered from snapshot because transaction history is not recovered.
    pub async fn next_priority_id(&mut self) -> PriorityOpId {
        {
            sqlx::query!(
                r#"
                SELECT
                    MAX(priority_op_id) AS "op_id"
                FROM
                    transactions
                WHERE
                    is_priority = TRUE
                    AND transactions.miniblock_number <= (
                        SELECT
                            MAX(number)
                        FROM
                            miniblocks
                    )
                "#
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .and_then(|row| row.op_id)
            .map(|value| PriorityOpId((value + 1) as u64))
            .unwrap_or_default()
        }
    }

    /// Returns L2 blocks with their transactions that state_keeper needs to re-execute on restart.
    /// These are the transactions that are included to some L2 block,
    /// but not included to L1 batch. The order of the transactions is the same as it was
    /// during the previous execution.
    pub async fn get_l2_blocks_to_reexecute(&mut self) -> DalResult<Vec<L2BlockExecutionData>> {
        let transactions = sqlx::query_as!(
            StorageTransaction,
            r#"
            SELECT
                *
            FROM
                transactions
            WHERE
                miniblock_number IS NOT NULL
                AND l1_batch_number IS NULL
            ORDER BY
                miniblock_number,
                index_in_block
            "#,
        )
        .instrument("get_l2_blocks_to_reexecute#transactions")
        .fetch_all(self.storage)
        .await?;

        self.map_transactions_to_execution_data(transactions, None)
            .await
    }

    /// Returns L2 blocks with their transactions to be used in VM execution.
    /// The order of the transactions is the same as it was during previous execution.
    /// All L2 blocks are retrieved for the given l1_batch.
    pub async fn get_l2_blocks_to_execute_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Vec<L2BlockExecutionData>> {
        let transactions = sqlx::query_as!(
            StorageTransaction,
            r#"
            SELECT
                *
            FROM
                transactions
            WHERE
                miniblock_number BETWEEN (
                    SELECT
                        MIN(number)
                    FROM
                        miniblocks
                    WHERE
                        miniblocks.l1_batch_number = $1
                ) AND (
                    SELECT
                        MAX(number)
                    FROM
                        miniblocks
                    WHERE
                        miniblocks.l1_batch_number = $1
                )
            ORDER BY
                miniblock_number,
                index_in_block
            "#,
            i64::from(l1_batch_number.0)
        )
        .instrument("get_l2_blocks_to_execute_for_l1_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_all(self.storage)
        .await?;

        let fictive_l2_block = sqlx::query!(
            r#"
            SELECT
                number
            FROM
                miniblocks
            WHERE
                miniblocks.l1_batch_number = $1
                AND l1_tx_count = 0
                AND l2_tx_count = 0
            ORDER BY
                number
            "#,
            i64::from(l1_batch_number.0)
        )
        .instrument("get_l2_blocks_to_execute_for_l1_batch#fictive_l2_block")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_optional(self.storage)
        .await?
        .map(|row| L2BlockNumber(row.number as u32));

        self.map_transactions_to_execution_data(transactions, fictive_l2_block)
            .await
    }

    async fn map_transactions_to_execution_data(
        &mut self,
        transactions: Vec<StorageTransaction>,
        fictive_l2_block: Option<L2BlockNumber>,
    ) -> DalResult<Vec<L2BlockExecutionData>> {
        let mut transactions_by_l2_block: Vec<(L2BlockNumber, Vec<Transaction>)> = transactions
            .into_iter()
            .chunk_by(|tx| tx.miniblock_number.unwrap())
            .into_iter()
            .map(|(l2_block_number, txs)| {
                (
                    L2BlockNumber(l2_block_number as u32),
                    txs.map(Transaction::from).collect::<Vec<_>>(),
                )
            })
            .collect();
        // Fictive L2 block is always at the end of a batch so it is safe to append it
        if let Some(fictive_l2_block) = fictive_l2_block {
            transactions_by_l2_block.push((fictive_l2_block, Vec::new()));
        }
        if transactions_by_l2_block.is_empty() {
            return Ok(Vec::new());
        }
        let from_l2_block = transactions_by_l2_block.first().unwrap().0;
        let to_l2_block = transactions_by_l2_block.last().unwrap().0;
        // `unwrap()`s are safe; `transactions_by_l2_block` is not empty as checked above

        let l2_block_data = sqlx::query!(
            r#"
            SELECT
                timestamp,
                virtual_blocks
            FROM
                miniblocks
            WHERE
                number BETWEEN $1 AND $2
            ORDER BY
                number
            "#,
            i64::from(from_l2_block.0),
            i64::from(to_l2_block.0)
        )
        .instrument("map_transactions_to_execution_data#miniblocks")
        .with_arg("from_l2_block", &from_l2_block)
        .with_arg("to_l2_block", &to_l2_block)
        .fetch_all(self.storage)
        .await?;

        if l2_block_data.len() != transactions_by_l2_block.len() {
            let err = Instrumented::new("map_transactions_to_execution_data")
                .with_arg("transactions_by_l2_block", &transactions_by_l2_block)
                .with_arg("l2_block_data", &l2_block_data)
                .constraint_error(anyhow::anyhow!("not enough miniblock data retrieved"));
            return Err(err);
        }

        let prev_l2_block_hashes = sqlx::query!(
            r#"
            SELECT
                number,
                hash
            FROM
                miniblocks
            WHERE
                number BETWEEN $1 AND $2
            ORDER BY
                number
            "#,
            i64::from(from_l2_block.0) - 1,
            i64::from(to_l2_block.0) - 1,
        )
        .instrument("map_transactions_to_execution_data#prev_l2_block_hashes")
        .with_arg("from_l2_block", &(from_l2_block - 1))
        .with_arg("to_l2_block", &(to_l2_block - 1))
        .fetch_all(self.storage)
        .await?;

        let prev_l2_block_hashes: HashMap<_, _> = prev_l2_block_hashes
            .into_iter()
            .map(|row| {
                (
                    L2BlockNumber(row.number as u32),
                    H256::from_slice(&row.hash),
                )
            })
            .collect();

        let mut data = Vec::with_capacity(transactions_by_l2_block.len());
        let it = transactions_by_l2_block.into_iter().zip(l2_block_data);
        for ((number, txs), l2_block_row) in it {
            let prev_l2_block_number = number - 1;
            let prev_block_hash = match prev_l2_block_hashes.get(&prev_l2_block_number) {
                Some(hash) => *hash,
                None => {
                    // Can occur after snapshot recovery; the first previous L2 block may not be present
                    // in the storage.
                    let row = sqlx::query!(
                        r#"
                        SELECT
                            miniblock_hash
                        FROM
                            snapshot_recovery
                        WHERE
                            miniblock_number = $1
                        "#,
                        prev_l2_block_number.0 as i32
                    )
                    .instrument("map_transactions_to_execution_data#snapshot_recovery")
                    .with_arg("prev_l2_block_number", &prev_l2_block_number)
                    .fetch_one(self.storage)
                    .await?;
                    H256::from_slice(&row.miniblock_hash)
                }
            };

            data.push(L2BlockExecutionData {
                number,
                timestamp: l2_block_row.timestamp as u64,
                prev_block_hash,
                virtual_blocks: l2_block_row.virtual_blocks as u32,
                txs,
            });
        }
        Ok(data)
    }

    pub async fn get_call_trace(
        &mut self,
        tx_hash: H256,
    ) -> DalResult<Option<(Call, CallTraceMeta)>> {
        let row = sqlx::query!(
            r#"
            SELECT
                protocol_version,
                index_in_block,
                miniblocks.number AS "miniblock_number!",
                miniblocks.hash AS "miniblocks_hash!"
            FROM
                transactions
            INNER JOIN miniblocks ON transactions.miniblock_number = miniblocks.number
            WHERE
                transactions.hash = $1
            "#,
            tx_hash.as_bytes()
        )
        .instrument("get_call_trace")
        .with_arg("tx_hash", &tx_hash)
        .fetch_optional(self.storage)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let protocol_version = row
            .protocol_version
            .map(|v| (v as u16).try_into().unwrap())
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);

        Ok(sqlx::query!(
            r#"
            SELECT
                call_trace,
                transactions.error AS tx_error
            FROM
                call_traces
            INNER JOIN transactions ON tx_hash = transactions.hash
            WHERE
                tx_hash = $1
            "#,
            tx_hash.as_bytes()
        )
        .instrument("get_call_trace")
        .with_arg("tx_hash", &tx_hash)
        .fetch_optional(self.storage)
        .await?
        .map(|mut call_trace| {
            (
                parse_call_trace(&call_trace.call_trace, protocol_version),
                CallTraceMeta {
                    index_in_block: row.index_in_block.unwrap_or_default() as usize,
                    tx_hash,
                    block_number: row.miniblock_number as u32,
                    block_hash: H256::from_slice(&row.miniblocks_hash),
                    internal_error: call_trace.tx_error.take(),
                },
            )
        }))
    }

    pub(crate) async fn get_tx_by_hash(&mut self, hash: H256) -> DalResult<Option<Transaction>> {
        sqlx::query_as!(
            StorageTransaction,
            r#"
            SELECT
                *
            FROM
                transactions
            WHERE
                hash = $1
            "#,
            hash.as_bytes()
        )
        .map(Into::into)
        .instrument("get_tx_by_hash")
        .with_arg("hash", &hash)
        .fetch_optional(self.storage)
        .await
    }

    pub async fn get_storage_tx_by_hash(
        &mut self,
        hash: H256,
    ) -> DalResult<Option<StorageTransaction>> {
        sqlx::query_as!(
            StorageTransaction,
            r#"
            SELECT
                *
            FROM
                transactions
            WHERE
                hash = $1
            "#,
            hash.as_bytes()
        )
        .map(Into::into)
        .instrument("get_storage_tx_by_hash")
        .with_arg("hash", &hash)
        .fetch_optional(self.storage)
        .await
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::ProtocolVersion;

    use super::*;
    use crate::{
        tests::{create_l2_block_header, mock_execution_result, mock_l2_transaction},
        ConnectionPool, Core, CoreDal,
    };

    #[tokio::test]
    async fn getting_call_trace_for_transaction() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(1), L1BatchNumber(0))
            .await
            .unwrap();

        let tx = mock_l2_transaction();
        let tx_hash = tx.hash();
        conn.transactions_dal()
            .insert_transaction_l2(
                &tx,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        let mut tx_result = mock_execution_result(tx);
        tx_result.call_traces.push(Call {
            from: Address::from_low_u64_be(1),
            to: Address::from_low_u64_be(2),
            value: 100.into(),
            ..Call::default()
        });
        let expected_call_trace = tx_result.call_trace().unwrap();
        conn.transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                L2BlockNumber(1),
                &[tx_result],
                1.into(),
                ProtocolVersionId::latest(),
                false,
            )
            .await
            .unwrap();

        let (call_trace, _) = conn
            .transactions_dal()
            .get_call_trace(tx_hash)
            .await
            .unwrap()
            .expect("no call trace");
        assert_eq!(call_trace, expected_call_trace);
    }

    #[tokio::test]
    async fn insert_l2_block_executed_txs() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let tx = mock_l2_transaction();
        let tx_hash = tx.hash();
        let tx_result = mock_execution_result(tx);
        conn.transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                L2BlockNumber(1),
                &[tx_result],
                1.into(),
                ProtocolVersionId::latest(),
                true,
            )
            .await
            .unwrap();

        let tx_from_db = conn
            .transactions_web3_dal()
            .get_transactions(&[tx_hash], Default::default())
            .await
            .unwrap();
        assert_eq!(tx_from_db[0].hash, tx_hash);
    }
}
