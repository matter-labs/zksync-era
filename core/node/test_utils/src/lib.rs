//! Test utils.

use std::{collections::HashMap, ops};

use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_merkle_tree::{domain::ZkSyncTree, TreeInstruction};
use zksync_system_constants::{get_intrinsic_constants, ZKPORTER_IS_AVAILABLE};
use zksync_types::{
    block::{L1BatchHeader, L2BlockHasher, L2BlockHeader},
    commitment::{
        AuxCommitments, L1BatchCommitmentArtifacts, L1BatchCommitmentHash, L1BatchMetaParameters,
        L1BatchMetadata, PubdataParams,
    },
    fee::Fee,
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    l2::L2Tx,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    protocol_version::ProtocolSemanticVersion,
    settlement::SettlementLayer,
    snapshots::{SnapshotRecoveryStatus, SnapshotStorageLog},
    transaction_request::PaymasterParams,
    AccountTreeId, Address, K256PrivateKey, L1BatchNumber, L2BlockNumber, L2ChainId, Nonce,
    ProtocolVersion, ProtocolVersionId, StorageKey, StorageLog, H256, U256,
};
use zksync_vm_interface::{
    L1BatchEnv, L2BlockEnv, SystemEnv, TransactionExecutionResult, TxExecutionMode,
    TxExecutionStatus, VmExecutionMetrics,
};

/// Value for recent protocol versions.
const MAX_GAS_PER_PUBDATA_BYTE: u64 = 50_000;

/// Creates a mock system env with reasonable params.
pub fn default_system_env() -> SystemEnv {
    SystemEnv {
        zk_porter_available: ZKPORTER_IS_AVAILABLE,
        version: ProtocolVersionId::latest(),
        base_system_smart_contracts: BaseSystemContracts::load_from_disk(),
        bootloader_gas_limit: u32::MAX,
        execution_mode: TxExecutionMode::VerifyExecute,
        default_validation_computational_gas_limit: u32::MAX,
        chain_id: L2ChainId::from(270),
    }
}

/// Creates a mock L1 batch env with reasonable params.
pub fn default_l1_batch_env(number: u32, timestamp: u64, fee_account: Address) -> L1BatchEnv {
    L1BatchEnv {
        previous_batch_hash: None,
        number: L1BatchNumber(number),
        timestamp,
        fee_account,
        enforced_base_fee: None,
        first_l2_block: L2BlockEnv {
            number,
            timestamp,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(number - 1)),
            max_virtual_blocks_to_create: 1,
            interop_roots: vec![],
        },
        fee_input: BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
            fair_l2_gas_price: 1,
            fair_pubdata_price: 1,
            l1_gas_price: 1,
        }),
        settlement_layer: SettlementLayer::L1(zksync_types::SLChainId(79)),
    }
}

pub fn fake_rolling_txs_hash_for_block(number: u32) -> H256 {
    H256::from_low_u64_be(number.into())
}

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
        gas_per_pubdata_limit: MAX_GAS_PER_PUBDATA_BYTE,
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 1,
        gas_limit: 0,
        logs_bloom: Default::default(),
        pubdata_params: PubdataParams::genesis(),
        rolling_txs_hash: Some(fake_rolling_txs_hash_for_block(number)),
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
            evm_emulator: None,
        },
        ProtocolVersionId::latest(),
        SettlementLayer::for_tests(),
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
            evm_emulator_code_hash: BaseSystemContractsHashes::default().evm_emulator,
            protocol_version: Some(ProtocolVersionId::latest()),
        },
        aux_data_hash: H256::zero(),
        meta_parameters_hash: H256::zero(),
        pass_through_data_hash: H256::zero(),
        events_queue_commitment: Some(H256::zero()),
        bootloader_initial_content_commitment: Some(H256::zero()),
        state_diffs_compressed: vec![],
        state_diff_hash: Some(H256::zero()),
        local_root: Some(H256::zero()),
        aggregation_root: Some(H256::zero()),
        da_inclusion_data: Some(vec![]),
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
        local_root: metadata.local_root.unwrap(),
        aggregation_root: metadata.aggregation_root.unwrap(),
        state_diff_hash: metadata.state_diff_hash.unwrap(),
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
        Some(Address::random()),
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
        execution_info: VmExecutionMetrics::default(),
        execution_status: TxExecutionStatus::Success,
        refunded_gas: 0,
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
        contracts: &BaseSystemContracts,
        protocol_version: ProtocolVersionId,
    ) -> Self {
        let l1_batch = L1BatchHeader::new(
            l1_batch,
            l1_batch.0.into(),
            contracts.hashes(),
            protocol_version,
            SettlementLayer::for_tests(),
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
            gas_per_pubdata_limit: MAX_GAS_PER_PUBDATA_BYTE,
            base_system_contracts_hashes: contracts.hashes(),
            protocol_version: Some(protocol_version),
            virtual_blocks: 1,
            gas_limit: 0,
            logs_bloom: Default::default(),
            pubdata_params: PubdataParams::genesis(),
            rolling_txs_hash: Some(H256::zero()),
        };
        Snapshot {
            l1_batch,
            l2_block,
            factory_deps: [&contracts.bootloader, &contracts.default_aa]
                .into_iter()
                .chain(contracts.evm_emulator.as_ref())
                .map(|c| (c.hash, c.code.clone()))
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
    let snapshot = Snapshot::new(
        l1_batch,
        l2_block,
        storage_logs,
        &BaseSystemContracts::load_from_disk(),
        ProtocolVersionId::latest(),
    );
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
        .insert_soft_pruning_log(snapshot.l1_batch.number, snapshot.l2_block.number)
        .await
        .unwrap();
    storage
        .pruning_dal()
        .insert_hard_pruning_log(
            snapshot.l1_batch.number,
            snapshot.l2_block.number,
            snapshot_recovery.l1_batch_root_hash,
        )
        .await
        .unwrap();
    storage.commit().await.unwrap();
    snapshot_recovery
}

/// Inserts initial writes for the specified L1 batch based on storage logs.
pub async fn insert_initial_writes_for_batch(
    connection: &mut Connection<'_, Core>,
    l1_batch_number: L1BatchNumber,
) {
    let mut written_non_zero_slots: Vec<_> = connection
        .storage_logs_dal()
        .get_touched_slots_for_executed_l1_batch(l1_batch_number)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|(key, value)| (!value.is_zero()).then_some(key))
        .collect();
    written_non_zero_slots.sort_unstable();

    let hashed_keys: Vec<_> = written_non_zero_slots
        .iter()
        .map(|key| key.hashed_key())
        .collect();
    let pre_written_slots = connection
        .storage_logs_dedup_dal()
        .filter_written_slots(&hashed_keys)
        .await
        .unwrap();

    let keys_to_insert: Vec<_> = written_non_zero_slots
        .into_iter()
        .filter(|key| !pre_written_slots.contains(&key.hashed_key()))
        .map(|key| key.hashed_key())
        .collect();
    connection
        .storage_logs_dedup_dal()
        .insert_initial_writes(l1_batch_number, &keys_to_insert)
        .await
        .unwrap();
}

/// Generates storage logs using provided indices as seeds.
pub fn generate_storage_logs(indices: ops::Range<u32>) -> Vec<StorageLog> {
    // Addresses and keys of storage logs must be sorted for the `multi_block_workflow` test.
    let mut accounts = [
        "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2",
        "ef4bb7b21c5fe7432a7d63876cc59ecc23b46636",
        "89b8988a018f5348f52eeac77155a793adf03ecc",
        "782806db027c08d36b2bed376b4271d1237626b3",
        "b2b57b76717ee02ae1327cc3cf1f40e76f692311",
    ]
    .map(|s| AccountTreeId::new(s.parse::<Address>().unwrap()));
    accounts.sort_unstable();

    let account_keys = (indices.start / 5)..(indices.end / 5);
    let proof_keys = accounts.iter().flat_map(|&account| {
        account_keys
            .clone()
            .map(move |i| StorageKey::new(account, H256::from_low_u64_be(i.into())))
    });
    let proof_values = indices.map(|i| H256::from_low_u64_be(i.into()));

    let logs: Vec<_> = proof_keys
        .zip(proof_values)
        .map(|(proof_key, proof_value)| StorageLog::new_write_log(proof_key, proof_value))
        .collect();
    for window in logs.windows(2) {
        let [prev, next] = window else { unreachable!() };
        assert!(prev.key < next.key);
    }
    logs
}
