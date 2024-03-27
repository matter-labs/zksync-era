use std::{collections::HashMap, fmt, time::Duration};

use anyhow::Context as _;
use bigdecimal::BigDecimal;
use itertools::Itertools;
use sqlx::{error, types::chrono::NaiveDateTime};
use zksync_db_connection::{
    connection::Connection, instrument::InstrumentExt, utils::pg_interval_from_duration,
};
use zksync_types::{
    block::MiniblockExecutionData,
    fee::TransactionExecutionMetrics,
    l1::L1Tx,
    l2::L2Tx,
    protocol_upgrade::ProtocolUpgradeTx,
    tx::{tx_execution_info::TxExecutionStatus, TransactionExecutionResult},
    vm_trace::Call,
    Address, ExecuteTransactionCommon, L1BatchNumber, L1BlockNumber, MiniblockNumber, PriorityOpId,
    Transaction, H256, PROTOCOL_UPGRADE_TX_TYPE, U256,
};
use zksync_utils::u256_to_big_decimal;

use crate::{
    models::storage_transaction::{CallTrace, StorageTransaction},
    Core,
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

type TxLocations = Vec<(MiniblockNumber, Vec<(H256, u32, u16)>)>;

impl TransactionsDal<'_, '_> {
    pub async fn insert_transaction_l1(&mut self, tx: L1Tx, l1_block_number: L1BlockNumber) {
        {
            let contract_address = tx.execute.contract_address.as_bytes();
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

            let secs = (tx.received_timestamp_ms / 1000) as i64;
            let nanosecs = ((tx.received_timestamp_ms % 1000) * 1_000_000) as u32;
            #[allow(deprecated)]
            let received_at = NaiveDateTime::from_timestamp_opt(secs, nanosecs).unwrap();

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
                        $18,
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
                contract_address,
                l1_block_number.0 as i32,
                value,
                empty_address.as_bytes(),
                &[] as &[u8],
                tx_format,
                to_mint,
                refund_recipient,
                received_at,
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap();
        }
    }

    pub async fn insert_system_transaction(&mut self, tx: ProtocolUpgradeTx) {
        {
            let contract_address = tx.execute.contract_address.as_bytes().to_vec();
            let tx_hash = tx.common_data.hash().0.to_vec();
            let json_data = serde_json::to_value(&tx.execute).unwrap_or_else(|_| {
                panic!("cannot serialize tx {:?} to json", tx.common_data.hash())
            });
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

            let secs = (tx.received_timestamp_ms / 1000) as i64;
            let nanosecs = ((tx.received_timestamp_ms % 1000) * 1_000_000) as u32;

            #[allow(deprecated)]
            let received_at = NaiveDateTime::from_timestamp_opt(secs, nanosecs).unwrap();

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
                        $16,
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
                contract_address,
                l1_block_number,
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
        }
    }

    pub async fn insert_transaction_l2(
        &mut self,
        tx: L2Tx,
        exec_info: TransactionExecutionMetrics,
    ) -> sqlx::Result<L2TxSubmissionResult> {
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
        .fetch_optional(self.storage.conn())
        .await?
        .is_some();

        if is_duplicate {
            tracing::debug!("Prevented inserting duplicate L2 transaction {tx_hash:?} to DB");
            return Ok(L2TxSubmissionResult::Duplicate);
        }

        let initiator_address = tx.initiator_account();
        let contract_address = tx.execute.contract_address.as_bytes();
        let json_data = serde_json::to_value(&tx.execute)
            .unwrap_or_else(|_| panic!("cannot serialize tx {:?} to json", tx.hash()));
        let gas_limit = u256_to_big_decimal(tx.common_data.fee.gas_limit);
        let max_fee_per_gas = u256_to_big_decimal(tx.common_data.fee.max_fee_per_gas);
        let max_priority_fee_per_gas =
            u256_to_big_decimal(tx.common_data.fee.max_priority_fee_per_gas);
        let gas_per_pubdata_limit = u256_to_big_decimal(tx.common_data.fee.gas_per_pubdata_limit);
        let tx_format = tx.common_data.transaction_type as i32;
        let signature = tx.common_data.signature;
        let nonce = i64::from(tx.common_data.nonce.0);
        let input_data = tx.common_data.input.expect("Data is mandatory").data;
        let value = u256_to_big_decimal(tx.execute.value);
        let paymaster = tx.common_data.paymaster_params.paymaster.0.as_ref();
        let paymaster_input = tx.common_data.paymaster_params.paymaster_input;
        let secs = (tx.received_timestamp_ms / 1000) as i64;
        let nanosecs = ((tx.received_timestamp_ms % 1000) * 1_000_000) as u32;
        #[allow(deprecated)]
        let received_at = NaiveDateTime::from_timestamp_opt(secs, nanosecs).unwrap();
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
                    JSONB_BUILD_OBJECT('gas_used', $16::BIGINT, 'storage_writes', $17::INT, 'contracts_used', $18::INT),
                    $19,
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
                execution_info = JSONB_BUILD_OBJECT('gas_used', $16::BIGINT, 'storage_writes', $17::INT, 'contracts_used', $18::INT),
                in_mempool = FALSE,
                received_at = $19,
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
                // API servers, that simultaneously start execute it and try to inserted to DB)
                if let error::Error::Database(ref error) = err {
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
        block_number: L1BatchNumber,
        transactions: &[TransactionExecutionResult],
    ) {
        {
            let hashes: Vec<_> = transactions.iter().map(|tx| tx.hash.as_bytes()).collect();
            let l1_batch_tx_indexes: Vec<_> = (0..transactions.len() as i32).collect();
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
                            UNNEST($1::INT[]) AS l1_batch_tx_index,
                            UNNEST($2::bytea[]) AS hash
                    ) AS data_table
                WHERE
                    transactions.hash = data_table.hash
                "#,
                &l1_batch_tx_indexes,
                &hashes as &[&[u8]],
                i64::from(block_number.0)
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        }
    }

    pub async fn mark_txs_as_executed_in_miniblock(
        &mut self,
        miniblock_number: MiniblockNumber,
        transactions: &[TransactionExecutionResult],
        block_base_fee_per_gas: U256,
    ) {
        {
            let mut transaction = self.storage.start_transaction().await.unwrap();
            let mut l1_hashes = Vec::with_capacity(transactions.len());
            let mut l1_indices_in_block = Vec::with_capacity(transactions.len());
            let mut l1_errors = Vec::with_capacity(transactions.len());
            let mut l1_execution_infos = Vec::with_capacity(transactions.len());
            let mut l1_refunded_gas = Vec::with_capacity(transactions.len());
            let mut l1_effective_gas_prices = Vec::with_capacity(transactions.len());

            let mut upgrade_hashes = Vec::new();
            let mut upgrade_indices_in_block = Vec::new();
            let mut upgrade_errors = Vec::new();
            let mut upgrade_execution_infos = Vec::new();
            let mut upgrade_refunded_gas = Vec::new();
            let mut upgrade_effective_gas_prices = Vec::new();

            let mut l2_hashes = Vec::with_capacity(transactions.len());
            let mut l2_values = Vec::with_capacity(transactions.len());
            let mut l2_contract_addresses = Vec::with_capacity(transactions.len());
            let mut l2_paymaster = Vec::with_capacity(transactions.len());
            let mut l2_paymaster_input = Vec::with_capacity(transactions.len());
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

            let mut call_traces_tx_hashes = Vec::with_capacity(transactions.len());
            let mut bytea_call_traces = Vec::with_capacity(transactions.len());
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

                    if let Some(call_trace) = tx_res.call_trace() {
                        bytea_call_traces.push(bincode::serialize(&call_trace).unwrap());
                        call_traces_tx_hashes.push(hash.0.to_vec());
                    }

                    match &transaction.common_data {
                        ExecuteTransactionCommon::L1(common_data) => {
                            l1_hashes.push(hash.0.to_vec());
                            l1_indices_in_block.push(index_in_block as i32);
                            l1_errors.push(error.unwrap_or_default());
                            l1_execution_infos.push(serde_json::to_value(execution_info).unwrap());
                            l1_refunded_gas.push(i64::from(*refunded_gas));
                            l1_effective_gas_prices
                                .push(u256_to_big_decimal(common_data.max_fee_per_gas));
                        }
                        ExecuteTransactionCommon::L2(common_data) => {
                            let data = serde_json::to_value(&transaction.execute).unwrap();
                            l2_values.push(u256_to_big_decimal(transaction.execute.value));
                            l2_contract_addresses
                                .push(transaction.execute.contract_address.as_bytes().to_vec());
                            l2_paymaster_input
                                .push(common_data.paymaster_params.paymaster_input.clone());
                            l2_paymaster
                                .push(common_data.paymaster_params.paymaster.as_bytes().to_vec());
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
                            l2_refunded_gas.push(i64::from(*refunded_gas));
                        }
                        ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                            upgrade_hashes.push(hash.0.to_vec());
                            upgrade_indices_in_block.push(index_in_block as i32);
                            upgrade_errors.push(error.unwrap_or_default());
                            upgrade_execution_infos
                                .push(serde_json::to_value(execution_info).unwrap());
                            upgrade_refunded_gas.push(i64::from(*refunded_gas));
                            upgrade_effective_gas_prices
                                .push(u256_to_big_decimal(common_data.max_fee_per_gas));
                        }
                    }
                });

            if !l2_hashes.is_empty() {
                // Update L2 txs

                // Due to the current tx replacement model, it's possible that tx has been replaced,
                // but the original was executed in memory,
                // so we have to update all fields for tx from fields stored in memory.
                // Note, that transactions are updated in order of their hashes to avoid deadlocks with other UPDATE queries.
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
                                        UNNEST($1::bytea[]) AS initiator_address,
                                        UNNEST($2::INT[]) AS nonce,
                                        UNNEST($3::bytea[]) AS hash,
                                        UNNEST($4::bytea[]) AS signature,
                                        UNNEST($5::NUMERIC[]) AS gas_limit,
                                        UNNEST($6::NUMERIC[]) AS max_fee_per_gas,
                                        UNNEST($7::NUMERIC[]) AS max_priority_fee_per_gas,
                                        UNNEST($8::NUMERIC[]) AS gas_per_pubdata_limit,
                                        UNNEST($9::INT[]) AS tx_format,
                                        UNNEST($10::INTEGER[]) AS index_in_block,
                                        UNNEST($11::VARCHAR[]) AS error,
                                        UNNEST($12::NUMERIC[]) AS effective_gas_price,
                                        UNNEST($13::jsonb[]) AS new_execution_info,
                                        UNNEST($14::bytea[]) AS input,
                                        UNNEST($15::jsonb[]) AS data,
                                        UNNEST($16::BIGINT[]) AS refunded_gas,
                                        UNNEST($17::NUMERIC[]) AS value,
                                        UNNEST($18::bytea[]) AS contract_address,
                                        UNNEST($19::bytea[]) AS paymaster,
                                        UNNEST($20::bytea[]) AS paymaster_input
                                ) AS data_table_temp
                                JOIN transactions ON transactions.initiator_address = data_table_temp.initiator_address
                                AND transactions.nonce = data_table_temp.nonce
                            ORDER BY
                                transactions.hash
                        ) AS data_table
                    WHERE
                        transactions.initiator_address = data_table.initiator_address
                        AND transactions.nonce = data_table.nonce
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
                    &l2_inputs as &[&[u8]],
                    &l2_datas,
                    &l2_refunded_gas,
                    &l2_values,
                    &l2_contract_addresses,
                    &l2_paymaster,
                    &l2_paymaster_input,
                    miniblock_number.0 as i32,
                )
                .execute(transaction.conn())
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
                        in_mempool = FALSE,
                        execution_info = execution_info || data_table.new_execution_info,
                        refunded_gas = data_table.refunded_gas,
                        effective_gas_price = data_table.effective_gas_price,
                        updated_at = NOW()
                    FROM
                        (
                            SELECT
                                UNNEST($2::bytea[]) AS hash,
                                UNNEST($3::INTEGER[]) AS index_in_block,
                                UNNEST($4::VARCHAR[]) AS error,
                                UNNEST($5::jsonb[]) AS new_execution_info,
                                UNNEST($6::BIGINT[]) AS refunded_gas,
                                UNNEST($7::NUMERIC[]) AS effective_gas_price
                        ) AS data_table
                    WHERE
                        transactions.hash = data_table.hash
                    "#,
                    miniblock_number.0 as i32,
                    &l1_hashes,
                    &l1_indices_in_block,
                    &l1_errors,
                    &l1_execution_infos,
                    &l1_refunded_gas,
                    &l1_effective_gas_prices,
                )
                .execute(transaction.conn())
                .await
                .unwrap();
            }

            if !upgrade_hashes.is_empty() {
                sqlx::query!(
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
                                UNNEST($2::bytea[]) AS hash,
                                UNNEST($3::INTEGER[]) AS index_in_block,
                                UNNEST($4::VARCHAR[]) AS error,
                                UNNEST($5::jsonb[]) AS new_execution_info,
                                UNNEST($6::BIGINT[]) AS refunded_gas,
                                UNNEST($7::NUMERIC[]) AS effective_gas_price
                        ) AS data_table
                    WHERE
                        transactions.hash = data_table.hash
                    "#,
                    miniblock_number.0 as i32,
                    &upgrade_hashes,
                    &upgrade_indices_in_block,
                    &upgrade_errors,
                    &upgrade_execution_infos,
                    &upgrade_refunded_gas,
                    &upgrade_effective_gas_prices,
                )
                .execute(transaction.conn())
                .await
                .unwrap();
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
                        UNNEST($1::bytea[], $2::bytea[]) AS u (tx_hash, call_trace)
                    "#,
                    &call_traces_tx_hashes,
                    &bytea_call_traces
                )
                .instrument("insert_call_tracer")
                .report_latency()
                .execute(&mut transaction)
                .await
                .unwrap();
            }
            transaction.commit().await.unwrap();
        }
    }

    pub async fn mark_tx_as_rejected(&mut self, transaction_hash: H256, error: &str) {
        {
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
                transaction_hash.0.to_vec()
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        }
    }

    pub async fn reset_transactions_state(&mut self, miniblock_number: MiniblockNumber) {
        {
            let tx_hashes = sqlx::query!(
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
                i64::from(miniblock_number.0)
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            sqlx::query!(
                r#"
                DELETE FROM call_traces
                WHERE
                    tx_hash = ANY ($1)
                "#,
                &tx_hashes
                    .iter()
                    .map(|tx| tx.hash.clone())
                    .collect::<Vec<Vec<u8>>>()
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        }
    }

    pub async fn remove_stuck_txs(&mut self, stuck_tx_timeout: Duration) -> sqlx::Result<usize> {
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
        .fetch_all(self.storage.conn())
        .await?;

        Ok(rows.len())
    }

    /// Fetches new updates for mempool. Returns new transactions and current nonces for related accounts;
    /// the latter are only used to bootstrap mempool for given account.
    pub async fn sync_mempool(
        &mut self,
        stashed_accounts: &[Address],
        purged_accounts: &[Address],
        gas_per_pubdata: u32,
        fee_per_gas: u64,
        limit: usize,
    ) -> sqlx::Result<Vec<Transaction>> {
        let stashed_addresses: Vec<_> = stashed_accounts.iter().map(Address::as_bytes).collect();
        sqlx::query!(
            r#"
            UPDATE transactions
            SET
                in_mempool = FALSE
            FROM
                UNNEST($1::bytea[]) AS s (address)
            WHERE
                transactions.in_mempool = TRUE
                AND transactions.initiator_address = s.address
            "#,
            &stashed_addresses as &[&[u8]],
        )
        .execute(self.storage.conn())
        .await?;

        let purged_addresses: Vec<_> = purged_accounts.iter().map(Address::as_bytes).collect();
        sqlx::query!(
            r#"
            DELETE FROM transactions
            WHERE
                in_mempool = TRUE
                AND initiator_address = ANY ($1)
            "#,
            &purged_addresses as &[&[u8]]
        )
        .execute(self.storage.conn())
        .await?;

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
                                    is_priority = TRUE
                                    OR (
                                        max_fee_per_gas >= $2
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
            i32::from(PROTOCOL_UPGRADE_TX_TYPE)
        )
        .fetch_all(self.storage.conn())
        .await?;

        let transactions = transactions.into_iter().map(|tx| tx.into()).collect();
        Ok(transactions)
    }

    pub async fn reset_mempool(&mut self) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE transactions
            SET
                in_mempool = FALSE
            WHERE
                in_mempool = TRUE
            "#
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_last_processed_l1_block(&mut self) -> Option<L1BlockNumber> {
        {
            sqlx::query!(
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
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .and_then(|x| x.l1_block_number.map(|block| L1BlockNumber(block as u32)))
        }
    }

    pub async fn last_priority_id(&mut self) -> Option<PriorityOpId> {
        {
            let op_id = sqlx::query!(
                r#"
                SELECT
                    MAX(priority_op_id) AS "op_id"
                FROM
                    transactions
                WHERE
                    is_priority = TRUE
                "#
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()?
            .op_id?;
            Some(PriorityOpId(op_id as u64))
        }
    }

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
                    AND miniblock_number IS NOT NULL
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

    /// Returns miniblocks with their transactions that state_keeper needs to re-execute on restart.
    /// These are the transactions that are included to some miniblock,
    /// but not included to L1 batch. The order of the transactions is the same as it was
    /// during the previous execution.
    pub async fn get_miniblocks_to_reexecute(
        &mut self,
    ) -> anyhow::Result<Vec<MiniblockExecutionData>> {
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
        .fetch_all(self.storage.conn())
        .await?;

        self.map_transactions_to_execution_data(transactions).await
    }

    /// Returns miniblocks with their transactions to be used in VM execution.
    /// The order of the transactions is the same as it was during previous execution.
    /// All miniblocks are retrieved for the given l1_batch.
    pub async fn get_miniblocks_to_execute_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Vec<MiniblockExecutionData>> {
        let transactions = sqlx::query_as!(
            StorageTransaction,
            r#"
            SELECT
                *
            FROM
                transactions
            WHERE
                l1_batch_number = $1
            ORDER BY
                miniblock_number,
                index_in_block
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_all(self.storage.conn())
        .await?;

        self.map_transactions_to_execution_data(transactions).await
    }

    async fn map_transactions_to_execution_data(
        &mut self,
        transactions: Vec<StorageTransaction>,
    ) -> anyhow::Result<Vec<MiniblockExecutionData>> {
        let transactions_by_miniblock: Vec<(MiniblockNumber, Vec<Transaction>)> = transactions
            .into_iter()
            .group_by(|tx| tx.miniblock_number.unwrap())
            .into_iter()
            .map(|(miniblock_number, txs)| {
                (
                    MiniblockNumber(miniblock_number as u32),
                    txs.map(Transaction::from).collect::<Vec<_>>(),
                )
            })
            .collect();
        if transactions_by_miniblock.is_empty() {
            return Ok(Vec::new());
        }
        let from_miniblock = transactions_by_miniblock.first().unwrap().0;
        let to_miniblock = transactions_by_miniblock.last().unwrap().0;
        // `unwrap()`s are safe; `transactions_by_miniblock` is not empty as checked above

        let miniblock_data = sqlx::query!(
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
            i64::from(from_miniblock.0),
            i64::from(to_miniblock.0)
        )
        .fetch_all(self.storage.conn())
        .await?;

        anyhow::ensure!(
            miniblock_data.len() == transactions_by_miniblock.len(),
            "Not enough miniblock data retrieved"
        );

        let prev_miniblock_hashes = sqlx::query!(
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
            i64::from(from_miniblock.0) - 1,
            i64::from(to_miniblock.0) - 1,
        )
        .fetch_all(self.storage.conn())
        .await?;

        let prev_miniblock_hashes: HashMap<_, _> = prev_miniblock_hashes
            .into_iter()
            .map(|row| {
                (
                    MiniblockNumber(row.number as u32),
                    H256::from_slice(&row.hash),
                )
            })
            .collect();

        let mut data = Vec::with_capacity(transactions_by_miniblock.len());
        let it = transactions_by_miniblock.into_iter().zip(miniblock_data);
        for ((number, txs), miniblock_row) in it {
            let prev_miniblock_number = number - 1;
            let prev_block_hash = match prev_miniblock_hashes.get(&prev_miniblock_number) {
                Some(hash) => *hash,
                None => {
                    // Can occur after snapshot recovery; the first previous miniblock may not be present
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
                        prev_miniblock_number.0 as i32
                    )
                    .fetch_optional(self.storage.conn())
                    .await?
                    .with_context(|| {
                        format!(
                            "miniblock #{prev_miniblock_number} is not in storage, and its hash is not \
                             in snapshot recovery data"
                        )
                    })?;
                    H256::from_slice(&row.miniblock_hash)
                }
            };

            data.push(MiniblockExecutionData {
                number,
                timestamp: miniblock_row.timestamp as u64,
                prev_block_hash,
                virtual_blocks: miniblock_row.virtual_blocks as u32,
                txs,
            });
        }
        Ok(data)
    }

    pub async fn get_tx_locations(&mut self, l1_batch_number: L1BatchNumber) -> TxLocations {
        {
            sqlx::query!(
                r#"
                SELECT
                    miniblock_number AS "miniblock_number!",
                    hash,
                    index_in_block AS "index_in_block!",
                    l1_batch_tx_index AS "l1_batch_tx_index!"
                FROM
                    transactions
                WHERE
                    l1_batch_number = $1
                ORDER BY
                    miniblock_number,
                    index_in_block
                "#,
                i64::from(l1_batch_number.0)
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
                    rows.map(|row| {
                        (
                            H256::from_slice(&row.hash),
                            row.index_in_block as u32,
                            row.l1_batch_tx_index as u16,
                        )
                    })
                    .collect::<Vec<(H256, u32, u16)>>(),
                )
            })
            .collect()
        }
    }

    pub async fn get_call_trace(&mut self, tx_hash: H256) -> sqlx::Result<Option<Call>> {
        Ok(sqlx::query_as!(
            CallTrace,
            r#"
            SELECT
                call_trace
            FROM
                call_traces
            WHERE
                tx_hash = $1
            "#,
            tx_hash.as_bytes()
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(Into::into))
    }

    pub(crate) async fn get_tx_by_hash(&mut self, hash: H256) -> Option<Transaction> {
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
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|tx| tx.into())
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::ProtocolVersion;

    use super::*;
    use crate::{
        tests::{create_miniblock_header, mock_execution_result, mock_l2_transaction},
        ConnectionPool, Core, CoreDal,
    };

    #[tokio::test]
    async fn getting_call_trace_for_transaction() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(1))
            .await
            .unwrap();

        let tx = mock_l2_transaction();
        let tx_hash = tx.hash();
        conn.transactions_dal()
            .insert_transaction_l2(tx.clone(), TransactionExecutionMetrics::default())
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
            .mark_txs_as_executed_in_miniblock(MiniblockNumber(1), &[tx_result], 1.into())
            .await;

        let call_trace = conn
            .transactions_dal()
            .get_call_trace(tx_hash)
            .await
            .unwrap()
            .expect("no call trace");
        assert_eq!(call_trace, expected_call_trace);
    }
}
