//! Test utils.

use std::collections::HashMap;

use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_merkle_tree::{domain::ZkSyncTree, TreeInstruction};
use zksync_multivm::utils::get_max_gas_per_pubdata_byte;
use zksync_node_genesis::GenesisParams;
use zksync_system_constants::{get_intrinsic_constants, ZKPORTER_IS_AVAILABLE};
use zksync_types::{
    block::{L1BatchHeader, L2BlockHeader},
    commitment::{
        AuxCommitments, L1BatchCommitmentArtifacts, L1BatchCommitmentHash, L1BatchMetaParameters,
        L1BatchMetadata,
    },
    fee::Fee,
    fee_model::BatchFeeInput,
    l2::L2Tx,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    protocol_version::ProtocolSemanticVersion,
    snapshots::{SnapshotRecoveryStatus, SnapshotStorageLog},
    transaction_request::PaymasterParams,
    tx::{tx_execution_info::TxExecutionStatus, ExecutionMetrics, TransactionExecutionResult},
    Address, K256PrivateKey, L1BatchNumber, L2BlockNumber, L2ChainId, Nonce, ProtocolVersion,
    ProtocolVersionId, StorageLog, H256, U256,
};

/// Creates an L2 block header with the specified number and deterministic contents.
pub fn create_l2_block(number: u32) -> L2BlockHeader {
    L2BlockHeader {
        number: L2BlockNumber(number),
        timestamp: number.into(),
        hash: H256::from_low_u64_be(number.into()),
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: 100,
        batch_fee_input: BatchFeeInput::l1_pegged(100, 100),
        fee_account_address: Address::zero(),
        gas_per_pubdata_limit: get_max_gas_per_pubdata_byte(ProtocolVersionId::latest().into()),
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 1,
        gas_limit: 0,
    }
}

/// Creates an L1 batch header with the specified number and deterministic contents.
pub fn create_l1_batch(number: u32) -> L1BatchHeader {
    let mut header = L1BatchHeader::new(
        L1BatchNumber(number),
        number.into(),
        BaseSystemContractsHashes {
            bootloader: H256::repeat_byte(1),
            default_aa: H256::repeat_byte(42),
            evm_simulator: H256::repeat_byte(43),
        },
        ProtocolVersionId::latest(),
    );
    header.l1_tx_count = 3;
    header.l2_tx_count = 5;
    header.l2_to_l1_logs.push(UserL2ToL1Log(L2ToL1Log {
        shard_id: 0,
        is_service: false,
        tx_number_in_block: 2,
        sender: Address::repeat_byte(2),
        key: H256::repeat_byte(3),
        value: H256::zero(),
    }));
    header.l2_to_l1_messages.push(vec![22; 22]);
    header.l2_to_l1_messages.push(vec![33; 33]);

    header
}

/// Creates metadata for an L1 batch with the specified number.
pub fn create_l1_batch_metadata(number: u32) -> L1BatchMetadata {
    L1BatchMetadata {
        root_hash: H256::from_low_u64_be(number.into()),
        rollup_last_leaf_index: u64::from(number) + 20,
        initial_writes_compressed: Some(vec![]),
        repeated_writes_compressed: Some(vec![]),
        commitment: H256::from_low_u64_be(number.into()),
        l2_l1_merkle_root: H256::from_low_u64_be(number.into()),
        block_meta_params: L1BatchMetaParameters {
            zkporter_is_available: ZKPORTER_IS_AVAILABLE,
            bootloader_code_hash: BaseSystemContractsHashes::default().bootloader,
            default_aa_code_hash: BaseSystemContractsHashes::default().default_aa,
            evm_simulator_code_hash: BaseSystemContractsHashes::default().evm_simulator,
            protocol_version: Some(ProtocolVersionId::latest()),
        },
        aux_data_hash: H256::zero(),
        meta_parameters_hash: H256::zero(),
        pass_through_data_hash: H256::zero(),
        events_queue_commitment: Some(H256::zero()),
        bootloader_initial_content_commitment: Some(H256::zero()),
        state_diffs_compressed: vec![],
    }
}

pub fn l1_batch_metadata_to_commitment_artifacts(
    metadata: &L1BatchMetadata,
) -> L1BatchCommitmentArtifacts {
    L1BatchCommitmentArtifacts {
        commitment_hash: L1BatchCommitmentHash {
            pass_through_data: metadata.pass_through_data_hash,
            aux_output: metadata.aux_data_hash,
            meta_parameters: metadata.meta_parameters_hash,
            commitment: metadata.commitment,
        },
        l2_l1_merkle_root: metadata.l2_l1_merkle_root,
        compressed_state_diffs: Some(metadata.state_diffs_compressed.clone()),
        compressed_initial_writes: metadata.initial_writes_compressed.clone(),
        compressed_repeated_writes: metadata.repeated_writes_compressed.clone(),
        zkporter_is_available: ZKPORTER_IS_AVAILABLE,
        aux_commitments: match (
            metadata.bootloader_initial_content_commitment,
            metadata.events_queue_commitment,
        ) {
            (Some(bootloader_initial_content_commitment), Some(events_queue_commitment)) => {
                Some(AuxCommitments {
                    bootloader_initial_content_commitment,
                    events_queue_commitment,
                })
            }
            _ => None,
        },
    }
}

/// Creates an L2 transaction with randomized parameters.
pub fn create_l2_transaction(fee_per_gas: u64, gas_per_pubdata: u64) -> L2Tx {
    let fee = Fee {
        gas_limit: (get_intrinsic_constants().l2_tx_intrinsic_gas * 2).into(),
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
        &K256PrivateKey::random(),
        vec![],
        PaymasterParams::default(),
    )
    .unwrap();
    // Input means all transaction data (NOT calldata, but all tx fields) that came from the API.
    // This input will be used for the derivation of the tx hash, so put some random to it to be sure
    // that the transaction hash is unique.
    tx.set_input(H256::random().0.to_vec(), H256::random());
    tx
}

pub fn execute_l2_transaction(transaction: L2Tx) -> TransactionExecutionResult {
    TransactionExecutionResult {
        hash: transaction.hash(),
        transaction: transaction.into(),
        execution_info: ExecutionMetrics::default(),
        execution_status: TxExecutionStatus::Success,
        refunded_gas: 0,
        operator_suggested_refund: 0,
        compressed_bytecodes: vec![],
        call_traces: vec![],
        revert_reason: None,
    }
}

/// Concise representation of a storage snapshot for testing recovery.
#[derive(Debug)]
pub struct Snapshot {
    pub l1_batch: L1BatchHeader,
    pub l2_block: L2BlockHeader,
    pub storage_logs: Vec<SnapshotStorageLog>,
    pub factory_deps: HashMap<H256, Vec<u8>>,
}

impl Snapshot {
    // Constructs a dummy Snapshot based on the provided values.
    pub fn new(
        l1_batch: L1BatchNumber,
        l2_block: L2BlockNumber,
        storage_logs: Vec<SnapshotStorageLog>,
        genesis_params: GenesisParams,
    ) -> Self {
        let contracts = genesis_params.base_system_contracts();
        let l1_batch = L1BatchHeader::new(
            l1_batch,
            l1_batch.0.into(),
            contracts.hashes(),
            genesis_params.minor_protocol_version(),
        );
        let l2_block = L2BlockHeader {
            number: l2_block,
            timestamp: l2_block.0.into(),
            hash: H256::from_low_u64_be(l2_block.0.into()),
            l1_tx_count: 0,
            l2_tx_count: 0,
            base_fee_per_gas: 100,
            batch_fee_input: BatchFeeInput::l1_pegged(100, 100),
            fee_account_address: Address::zero(),
            gas_per_pubdata_limit: get_max_gas_per_pubdata_byte(
                genesis_params.minor_protocol_version().into(),
            ),
            base_system_contracts_hashes: contracts.hashes(),
            protocol_version: Some(genesis_params.minor_protocol_version()),
            virtual_blocks: 1,
            gas_limit: 0,
        };
        Snapshot {
            l1_batch,
            l2_block,
            factory_deps: [&contracts.bootloader, &contracts.default_aa]
                .into_iter()
                .map(|c| (c.hash, zksync_utils::be_words_to_bytes(&c.code)))
                .collect(),
            storage_logs,
        }
    }
}

/// Prepares a recovery snapshot without performing genesis.
pub async fn prepare_recovery_snapshot(
    storage: &mut Connection<'_, Core>,
    l1_batch: L1BatchNumber,
    l2_block: L2BlockNumber,
    storage_logs: &[StorageLog],
) -> SnapshotRecoveryStatus {
    let storage_logs = storage_logs
        .iter()
        .enumerate()
        .map(|(i, log)| SnapshotStorageLog {
            key: log.key.hashed_key(),
            value: log.value,
            l1_batch_number_of_initial_write: l1_batch,
            enumeration_index: i as u64 + 1,
        })
        .collect();
    let snapshot = Snapshot::new(l1_batch, l2_block, storage_logs, GenesisParams::mock());
    recover(storage, snapshot).await
}

/// Takes a storage snapshot at the last sealed L1 batch.
pub async fn snapshot(storage: &mut Connection<'_, Core>) -> Snapshot {
    let l1_batch = storage
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .expect("no L1 batches in storage");
    let l1_batch = storage
        .blocks_dal()
        .get_l1_batch_header(l1_batch)
        .await
        .unwrap()
        .unwrap();
    let (_, l2_block) = storage
        .blocks_dal()
        .get_l2_block_range_of_l1_batch(l1_batch.number)
        .await
        .unwrap()
        .unwrap();
    let all_hashes = H256::zero()..=H256::repeat_byte(0xff);
    Snapshot {
        l2_block: storage
            .blocks_dal()
            .get_l2_block_header(l2_block)
            .await
            .unwrap()
            .unwrap(),
        storage_logs: storage
            .snapshots_creator_dal()
            .get_storage_logs_chunk(l2_block, l1_batch.number, all_hashes)
            .await
            .unwrap(),
        factory_deps: storage
            .snapshots_creator_dal()
            .get_all_factory_deps(l2_block)
            .await
            .unwrap()
            .into_iter()
            .collect(),
        l1_batch,
    }
}

/// Recovers storage from a snapshot.
/// L2 block and L1 batch are intentionally **not** inserted into the storage.
pub async fn recover(
    storage: &mut Connection<'_, Core>,
    snapshot: Snapshot,
) -> SnapshotRecoveryStatus {
    let mut storage = storage.start_transaction().await.unwrap();
    let written_keys: Vec<_> = snapshot.storage_logs.iter().map(|log| log.key).collect();
    let tree_instructions: Vec<_> = snapshot
        .storage_logs
        .iter()
        .map(|log| {
            let tree_key = U256::from_little_endian(log.key.as_bytes());
            TreeInstruction::write(tree_key, log.enumeration_index, log.value)
        })
        .collect();
    let l1_batch_root_hash = ZkSyncTree::process_genesis_batch(&tree_instructions).root_hash;

    let protocol_version = storage
        .protocol_versions_dal()
        .get_protocol_version_with_latest_patch(snapshot.l1_batch.protocol_version.unwrap())
        .await
        .unwrap();
    if let Some(protocol_version) = protocol_version {
        assert_eq!(
            protocol_version.base_system_contracts_hashes,
            snapshot.l1_batch.base_system_contracts_hashes,
            "Protocol version set up with incorrect base system contracts"
        );
    } else {
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion {
                base_system_contracts_hashes: snapshot.l1_batch.base_system_contracts_hashes,
                version: ProtocolSemanticVersion {
                    minor: snapshot.l1_batch.protocol_version.unwrap(),
                    patch: 0.into(),
                },
                ..ProtocolVersion::default()
            })
            .await
            .unwrap();
    }
    storage
        .factory_deps_dal()
        .insert_factory_deps(snapshot.l2_block.number, &snapshot.factory_deps)
        .await
        .unwrap();

    storage
        .storage_logs_dedup_dal()
        .insert_initial_writes(snapshot.l1_batch.number, &written_keys)
        .await
        .unwrap();
    storage
        .storage_logs_dal()
        .insert_storage_logs_from_snapshot(snapshot.l2_block.number, &snapshot.storage_logs)
        .await
        .unwrap();

    let snapshot_recovery = SnapshotRecoveryStatus {
        l1_batch_number: snapshot.l1_batch.number,
        l1_batch_timestamp: snapshot.l1_batch.timestamp,
        l1_batch_root_hash,
        l2_block_number: snapshot.l2_block.number,
        l2_block_timestamp: snapshot.l2_block.timestamp,
        l2_block_hash: snapshot.l2_block.hash,
        protocol_version: snapshot.l1_batch.protocol_version.unwrap(),
        storage_logs_chunks_processed: vec![true; 100],
    };
    storage
        .snapshot_recovery_dal()
        .insert_initial_recovery_status(&snapshot_recovery)
        .await
        .unwrap();

    storage
        .pruning_dal()
        .soft_prune_batches_range(snapshot.l1_batch.number, snapshot.l2_block.number)
        .await
        .unwrap();

    storage
        .pruning_dal()
        .hard_prune_batches_range(snapshot.l1_batch.number, snapshot.l2_block.number)
        .await
        .unwrap();

    storage.commit().await.unwrap();
    snapshot_recovery
}
