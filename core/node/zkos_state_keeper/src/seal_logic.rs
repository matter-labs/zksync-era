use std::collections::HashMap;

use zk_os_basic_system::basic_system::BasicBlockMetadataFromOracle;
use zk_os_forward_system::run::{BatchContext, BatchOutput, ExecutionResult, TxOutput};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_state_keeper::MempoolGuard;
use zksync_types::{
    block::{L1BatchHeader, L2BlockHeader},
    fee_model::{BatchFeeInput, L1PeggedBatchFeeModelInput, PubdataIndependentBatchFeeModelInput},
    snapshots::SnapshotStorageLog,
    tx::IncludedTxLocation,
    Address, L1BatchNumber, L2BlockNumber, ProtocolVersionId, Transaction, H256,
};
use zksync_vm_interface::VmEvent;
use zksync_zkos_vm_runner::zkos_conversions::{bytes32_to_h256, zkos_log_to_vm_event};

pub async fn seal_in_db<'a>(
    mut connection: Connection<'a, Core>,
    context: BatchContext,
    result: &BatchOutput,
    executed_transactions: Vec<Transaction>,
    block_hash: H256,
) -> anyhow::Result<()> {
    let l2_block_number = L2BlockNumber(context.block_number as u32);
    let l1_batch_number = L1BatchNumber(context.block_number as u32);

    let l1_batch_header = generate_l1_batch_header(context, l1_batch_number);
    let l2_block_header = generate_l2_block_header(context, l2_block_number);

    let mut transaction = connection.start_transaction().await?;

    let mut next_index_in_batch_output = 0;
    for tx in executed_transactions {
        let tx_hash = tx.hash();
        let internal_tx_result = loop {
            let internal_tx_result =
                extract_tx_internal_result(tx_hash, next_index_in_batch_output, &result);
            if matches!(internal_tx_result, InternalTxResult::Rejected(_)) {
                next_index_in_batch_output += 1;
            } else {
                break internal_tx_result;
            }
        };
        let gas_used = result.tx_results[next_index_in_batch_output]
            .as_ref()
            .unwrap()
            .gas_used;
        match internal_tx_result {
            InternalTxResult::Success => {
                tracing::info!("marking transaction {tx_hash:#?} as included");
                transaction
                    .transactions_dal()
                    .zkos_mark_tx_as_executed(tx_hash, l2_block_number, None, gas_used)
                    .await?;
            }
            InternalTxResult::Reverted(reason) => {
                tracing::info!("marking transaction {tx_hash:#?} as included");
                transaction
                    .transactions_dal()
                    .zkos_mark_tx_as_executed(tx_hash, l2_block_number, Some(reason), gas_used)
                    .await?;
            }
            InternalTxResult::Rejected(_reason) => {
                // it was already handled.
            }
        };
        next_index_in_batch_output += 1;
    }

    tracing::info!("inserting storage logs");
    let storage_logs = generate_storage_logs(result, l1_batch_number);
    tracing::info!("generated {} storage logs", storage_logs.len());
    transaction
        .storage_logs_dal()
        .insert_storage_logs_from_snapshot(l2_block_number, &storage_logs)
        .await?;

    tracing::info!("inserting factory deps");
    let factory_deps: HashMap<H256, Vec<u8>> = result
        .published_preimages
        .iter()
        .map(|(hash, bytecode)| (bytes32_to_h256(*hash), bytecode.clone()))
        .collect();
    transaction
        .factory_deps_dal()
        .insert_factory_deps(l2_block_number, &factory_deps)
        .await?;

    tracing::info!("inserted {} factory deps", factory_deps.len());

    // tracing::info!("inserting events");
    // let vm_events: Vec<VmEvent> = result
    //     .tx_results
    //     .clone()
    //     .into_iter()
    //     .map(|tx_result| tx_result.map(|a| a.logs).unwrap_or_default())
    //     .flatten()
    //     .map(|log| VmEvent {
    //         location: (l1_batch_number, 0), // we have 1 tx per batch, it's index is 0
    //         address: Address::from_slice(&log.address.to_be_bytes::<20>()),
    //         indexed_topics: log
    //             .topics
    //             .into_iter()
    //             .map(|topic| H256(topic.as_u8_array()))
    //             .collect(),
    //         value: log.data,
    //     })
    //     .collect();
    // let vm_events_ref: Vec<&VmEvent> = vm_events.iter().collect();
    // let events = [(
    //     IncludedTxLocation {
    //         tx_hash: executed_tx_hash.unwrap_or_default(),
    //         tx_index_in_l2_block: 0, // we have 1 tx per block, it's index is 0
    //     },
    //     vm_events_ref,
    // )];
    // transaction
    //     .events_dal()
    //     .save_events(l2_block_number, &events)
    //     .await?;
    //
    // tracing::info!("inserted {} events", vm_events.len());

    transaction
        .blocks_dal()
        .mark_l1_batch_as_sealed(&l1_batch_header, &[], &[], &[], Default::default())
        .await?;

    transaction
        .blocks_dal()
        .insert_l2_block(&l2_block_header)
        .await?;

    transaction
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_number)
        .await?;

    transaction.commit().await?;
    Ok(())
}

fn generate_storage_logs(
    result: &BatchOutput,
    l1_batch_number: L1BatchNumber,
) -> Vec<SnapshotStorageLog> {
    result
        .storage_writes
        .iter()
        .map(|storage_write| {
            let hashed_key = bytes32_to_h256(storage_write.key);
            let value = bytes32_to_h256(storage_write.value);
            SnapshotStorageLog {
                key: hashed_key,
                value,
                l1_batch_number_of_initial_write: l1_batch_number,
                enumeration_index: 0,
            }
        })
        .collect::<Vec<_>>()
}

fn generate_l1_batch_header(
    context: BasicBlockMetadataFromOracle,
    l1_batch_number: L1BatchNumber,
) -> L1BatchHeader {
    L1BatchHeader {
        number: l1_batch_number,
        timestamp: context.timestamp,
        priority_ops_onchain_data: Default::default(),
        l1_tx_count: 0,
        l2_tx_count: 1,
        l2_to_l1_logs: Default::default(),
        l2_to_l1_messages: Default::default(),
        bloom: Default::default(),
        used_contract_hashes: Default::default(),
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        system_logs: Default::default(),
        pubdata_input: Default::default(),
        fee_address: Default::default(),
        batch_fee_input: Default::default(),
    }
}

fn generate_l2_block_header(
    context: BasicBlockMetadataFromOracle,
    l2_block_number: L2BlockNumber,
) -> L2BlockHeader {
    L2BlockHeader {
        number: l2_block_number,
        timestamp: context.timestamp,
        hash: Default::default(), // todo
        l1_tx_count: 0,
        l2_tx_count: 1,
        fee_account_address: Default::default(), // todo
        base_fee_per_gas: context.eip1559_basefee.to(),
        batch_fee_input: Default::default(),
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        gas_per_pubdata_limit: u64::MAX,
        virtual_blocks: 0,
        gas_limit: u64::MAX,
        logs_bloom: Default::default(),
        pubdata_params: Default::default(),
    }
}

#[derive(Debug, Clone)]
enum InternalTxResult {
    Success,
    Rejected(String),
    Reverted(String),
}

fn extract_tx_internal_result(
    tx_hash: H256,
    tx_index_in_batch: usize,
    result: &BatchOutput,
) -> InternalTxResult {
    let tx_result = if let Some(tx_result) = result.tx_results.get(tx_index_in_batch).cloned() {
        tx_result
    } else {
        panic!("No tx result for {tx_hash} #{tx_index_in_batch}");
    };

    match tx_result {
        Ok(tx_output) => {
            if let ExecutionResult::Revert(revert_reason) = tx_output.execution_result {
                let mut str = "0x".to_string();
                str += &hex::encode(&revert_reason);
                InternalTxResult::Reverted(str)
            } else {
                InternalTxResult::Success
            }
        }
        Err(reason) => {
            tracing::error!("Invalid transaction, hash: {tx_hash:#?}, reason: {reason:?}");
            InternalTxResult::Rejected(format!(
                "Invalid transaction, hash: {tx_hash:#?}, reason: {reason:?}"
            ))
        }
    }
}

pub fn extract_tx_output(tx_index_in_batch: usize, result: &BatchOutput) -> Option<TxOutput> {
    let tx_result = if let Some(tx_result) = result.tx_results.get(tx_index_in_batch).cloned() {
        tx_result
    } else {
        panic!("No tx result for #{tx_index_in_batch}");
    };

    tx_result.ok()
}
