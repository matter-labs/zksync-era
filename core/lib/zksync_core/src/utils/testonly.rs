//! Test utils.

use multivm::utils::get_max_gas_per_pubdata_byte;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::StorageProcessor;
use zksync_merkle_tree::{domain::ZkSyncTree, TreeInstruction};
use zksync_system_constants::ZKPORTER_IS_AVAILABLE;
use zksync_types::{
    block::{L1BatchHeader, MiniblockHeader},
    commitment::{L1BatchMetaParameters, L1BatchMetadata},
    fee::Fee,
    fee_model::BatchFeeInput,
    l2::L2Tx,
    snapshots::SnapshotRecoveryStatus,
    transaction_request::PaymasterParams,
    Address, L1BatchNumber, L2ChainId, MiniblockNumber, Nonce, ProtocolVersion, ProtocolVersionId,
    StorageLog, H256, U256,
};

use crate::l1_gas_price::L1GasPriceProvider;

/// Creates a miniblock header with the specified number and deterministic contents.
pub(crate) fn create_miniblock(number: u32) -> MiniblockHeader {
    MiniblockHeader {
        number: MiniblockNumber(number),
        timestamp: number.into(),
        hash: H256::from_low_u64_be(number.into()),
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: 100,
        batch_fee_input: BatchFeeInput::l1_pegged(100, 100),
        gas_per_pubdata_limit: get_max_gas_per_pubdata_byte(ProtocolVersionId::latest().into()),
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 1,
    }
}

/// Creates an L1 batch header with the specified number and deterministic contents.
pub(crate) fn create_l1_batch(number: u32) -> L1BatchHeader {
    let mut header = L1BatchHeader::new(
        L1BatchNumber(number),
        number.into(),
        Address::default(),
        BaseSystemContractsHashes::default(),
        ProtocolVersionId::latest(),
    );
    header.is_finished = true;
    header
}

/// Creates metadata for an L1 batch with the specified number.
pub(crate) fn create_l1_batch_metadata(number: u32) -> L1BatchMetadata {
    L1BatchMetadata {
        root_hash: H256::from_low_u64_be(number.into()),
        rollup_last_leaf_index: u64::from(number) + 20,
        merkle_root_hash: H256::from_low_u64_be(number.into()),
        initial_writes_compressed: vec![],
        repeated_writes_compressed: vec![],
        commitment: H256::from_low_u64_be(number.into()),
        l2_l1_messages_compressed: vec![],
        l2_l1_merkle_root: H256::from_low_u64_be(number.into()),
        block_meta_params: L1BatchMetaParameters {
            zkporter_is_available: ZKPORTER_IS_AVAILABLE,
            bootloader_code_hash: BaseSystemContractsHashes::default().bootloader,
            default_aa_code_hash: BaseSystemContractsHashes::default().default_aa,
        },
        aux_data_hash: H256::zero(),
        meta_parameters_hash: H256::zero(),
        pass_through_data_hash: H256::zero(),
        events_queue_commitment: Some(H256::zero()),
        bootloader_initial_content_commitment: Some(H256::zero()),
        state_diffs_compressed: vec![],
    }
}

/// Creates an L2 transaction with randomized parameters.
pub(crate) fn create_l2_transaction(fee_per_gas: u64, gas_per_pubdata: u32) -> L2Tx {
    let fee = Fee {
        gas_limit: 1000_u64.into(),
        max_fee_per_gas: fee_per_gas.into(),
        max_priority_fee_per_gas: 0_u64.into(),
        gas_per_pubdata_limit: gas_per_pubdata.into(),
    };
    let mut tx = L2Tx::new_signed(
        Address::random(),
        vec![],
        Nonce(0),
        fee,
        U256::zero(),
        L2ChainId::from(271),
        &H256::random(),
        None,
        PaymasterParams::default(),
    )
    .unwrap();
    // Input means all transaction data (NOT calldata, but all tx fields) that came from the API.
    // This input will be used for the derivation of the tx hash, so put some random to it to be sure
    // that the transaction hash is unique.
    tx.set_input(H256::random().0.to_vec(), H256::random());
    tx
}

/// Prepares a recovery snapshot without performing genesis.
pub(crate) async fn prepare_recovery_snapshot(
    storage: &mut StorageProcessor<'_>,
    l1_batch_number: u32,
    snapshot_logs: &[StorageLog],
) -> SnapshotRecoveryStatus {
    let mut storage = storage.start_transaction().await.unwrap();

    let written_keys: Vec<_> = snapshot_logs.iter().map(|log| log.key).collect();
    let tree_instructions: Vec<_> = snapshot_logs
        .iter()
        .enumerate()
        .map(|(i, log)| TreeInstruction::write(log.key, i as u64 + 1, log.value))
        .collect();
    let l1_batch_root_hash = ZkSyncTree::process_genesis_batch(&tree_instructions).root_hash;

    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(ProtocolVersion::default())
        .await;
    // TODO (PLA-596): Don't insert L1 batches / miniblocks once the relevant foreign keys are removed
    let miniblock = create_miniblock(l1_batch_number);
    storage
        .blocks_dal()
        .insert_miniblock(&miniblock)
        .await
        .unwrap();
    let l1_batch = create_l1_batch(l1_batch_number);
    storage
        .blocks_dal()
        .insert_l1_batch(&l1_batch, &[], Default::default(), &[], &[], 0)
        .await
        .unwrap();

    storage
        .storage_logs_dedup_dal()
        .insert_initial_writes(l1_batch.number, &written_keys)
        .await;
    storage
        .storage_logs_dal()
        .insert_storage_logs(miniblock.number, &[(H256::zero(), snapshot_logs.to_vec())])
        .await;
    storage
        .storage_dal()
        .apply_storage_logs(&[(H256::zero(), snapshot_logs.to_vec())])
        .await;

    let snapshot_recovery = SnapshotRecoveryStatus {
        l1_batch_number: l1_batch.number,
        l1_batch_root_hash,
        miniblock_number: miniblock.number,
        miniblock_root_hash: H256::zero(), // not used
        last_finished_chunk_id: None,
        total_chunk_count: 100,
    };
    storage
        .snapshot_recovery_dal()
        .set_applied_snapshot_status(&snapshot_recovery)
        .await
        .unwrap();
    storage.commit().await.unwrap();
    snapshot_recovery
}

// TODO (PLA-596): Replace with `prepare_recovery_snapshot(.., &[])`
pub(crate) async fn prepare_empty_recovery_snapshot(
    storage: &mut StorageProcessor<'_>,
    l1_batch_number: u32,
) -> SnapshotRecoveryStatus {
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(ProtocolVersion::default())
        .await;

    let snapshot_recovery = SnapshotRecoveryStatus {
        l1_batch_number: l1_batch_number.into(),
        l1_batch_root_hash: H256::zero(),
        miniblock_number: l1_batch_number.into(),
        miniblock_root_hash: H256::zero(), // not used
        last_finished_chunk_id: None,
        total_chunk_count: 100,
    };
    storage
        .snapshot_recovery_dal()
        .set_applied_snapshot_status(&snapshot_recovery)
        .await
        .unwrap();
    snapshot_recovery
}

/// Mock [`L1GasPriceProvider`] that returns a constant value.
#[derive(Debug)]
pub(crate) struct MockL1GasPriceProvider(pub u64);

impl L1GasPriceProvider for MockL1GasPriceProvider {
    fn estimate_effective_gas_price(&self) -> u64 {
        self.0
    }

    fn estimate_effective_pubdata_price(&self) -> u64 {
        self.0 * u64::from(zksync_system_constants::L1_GAS_PER_PUBDATA_BYTE)
    }
}
