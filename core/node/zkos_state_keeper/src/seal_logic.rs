use zk_os_basic_system::basic_system::BasicBlockMetadataFromOracle;
use zk_os_forward_system::run::{BatchContext, BatchOutput};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::block::{L1BatchHeader, L2BlockHeader};
use zksync_types::{H256, L1BatchNumber, L2BlockNumber, ProtocolVersionId};
use zksync_types::fee_model::{BatchFeeInput, L1PeggedBatchFeeModelInput, PubdataIndependentBatchFeeModelInput};
use zksync_types::snapshots::SnapshotStorageLog;
use crate::zkos_conversions::bytes32_to_h256;

pub async fn seal_in_db<'a>(
    mut connection: Connection<'a, Core>,
    context: BatchContext,
    result: &BatchOutput,
    executed_tx_hash: H256,
    block_hash: H256,
) -> anyhow::Result<()> {

    let l2_block_number = L2BlockNumber(context.block_number as u32);
    let l1_batch_number = L1BatchNumber(context.block_number as u32);


    let l1_batch_header = generate_l1_batch_header(context, l1_batch_number);
    let l2_block_header = generate_l2_block_header(context, l2_block_number);


    let mut transaction =
        connection
            .start_transaction()
            .await?;

    tracing::info!("marking transaction as included");
    transaction
        .transactions_dal()
        .zkos_mark_tx_as_executed(
            executed_tx_hash,
            l2_block_number
        )
        .await?;

    tracing::info!("inserting storage logs");
    let storage_logs = generate_storage_logs(result, l1_batch_number);
    transaction
        .storage_logs_dal()
        .insert_storage_logs_from_snapshot(
            l2_block_number,
            &storage_logs
        )
        .await?;

    // todo :factory deps

    transaction
        .blocks_dal()
        .zkos_insert_sealed_l1_batch(&l1_batch_header)
        .await?;

    transaction
        .blocks_dal()
        .insert_l2_block(&l2_block_header)
        .await?;

    transaction.commit().await?;
    Ok(())
}

fn generate_storage_logs(result: &BatchOutput, l1_batch_number: L1BatchNumber) -> Vec<SnapshotStorageLog> {
    result.storage_writes.iter().map(|storage_write| {
        let hashed_key = bytes32_to_h256(storage_write.key);
        let value = bytes32_to_h256(storage_write.value);
        SnapshotStorageLog {
            key: hashed_key,
            value,
            l1_batch_number_of_initial_write: l1_batch_number,
            enumeration_index: 0,
        }
    }).collect::<Vec<_>>()
}

fn generate_l1_batch_header(context: BasicBlockMetadataFromOracle, l1_batch_number: L1BatchNumber) -> L1BatchHeader {
    L1BatchHeader {
        number: l1_batch_number,
        timestamp: context.timestamp,
        priority_ops_onchain_data: Default::default(),
        l1_tx_count: 0,
        l2_tx_count: 1,
        l2_to_l1_logs: Default::default(),
        l2_to_l1_messages: Default::default(),
        bloom: Default::default(),
        used_contract_hashes: Default::default(), // todo: where is this used?
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        system_logs: Default::default(),
        pubdata_input: Default::default(),
        fee_address: Default::default(),
    }
}

fn generate_l2_block_header(context: BasicBlockMetadataFromOracle, l2_block_number: L2BlockNumber) -> L2BlockHeader {
    L2BlockHeader {
        number: l2_block_number,
        timestamp: context.timestamp,
        hash: Default::default(), // todo
        l1_tx_count: 0,
        l2_tx_count: 1,
        fee_account_address: Default::default(), // todo
        base_fee_per_gas: 1,
        batch_fee_input: Default::default(),
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        gas_per_pubdata_limit: u64::MAX,
        virtual_blocks: 0,
        gas_limit: u64::MAX,
        logs_bloom: Default::default(),
    }
}