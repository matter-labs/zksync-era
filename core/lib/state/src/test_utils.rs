//! Shared utils for unit tests.

use std::ops;

use zksync_dal::{pruning_dal::HardPruningStats, Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{
    block::{L1BatchHeader, L2BlockHeader},
    commitment::PubdataParams,
    snapshots::SnapshotRecoveryStatus,
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, ProtocolVersion, ProtocolVersionId,
    StorageKey, StorageLog, H256,
};

pub(crate) async fn prepare_postgres(conn: &mut Connection<'_, Core>) {
    prepare_postgres_with_log_count(conn, 20).await;
}

pub(crate) async fn prepare_postgres_with_log_count(
    conn: &mut Connection<'_, Core>,
    log_count: u64,
) -> Vec<StorageLog> {
    assert!(conn.blocks_dal().is_genesis_needed().await.unwrap());
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    // The created genesis block is likely to be invalid, but since it's not committed,
    // we don't really care.
    let genesis_storage_logs = gen_storage_logs(0..log_count);
    create_l2_block(conn, L2BlockNumber(0), &genesis_storage_logs).await;
    create_l1_batch(conn, L1BatchNumber(0), &genesis_storage_logs).await;
    genesis_storage_logs
}

pub(crate) fn gen_storage_logs(indices: ops::Range<u64>) -> Vec<StorageLog> {
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
            .map(move |i| StorageKey::new(account, H256::from_low_u64_be(i)))
    });
    let proof_values = indices.map(|i| H256::from_low_u64_be(i + 1));

    proof_keys
        .zip(proof_values)
        .map(|(proof_key, proof_value)| StorageLog::new_write_log(proof_key, proof_value))
        .collect()
}

#[allow(clippy::default_trait_access)]
// ^ `BaseSystemContractsHashes::default()` would require a new direct dependency
pub(crate) async fn create_l2_block(
    conn: &mut Connection<'_, Core>,
    l2_block_number: L2BlockNumber,
    block_logs: &[StorageLog],
) {
    let l2_block_header = L2BlockHeader {
        number: l2_block_number,
        timestamp: 0,
        hash: H256::from_low_u64_be(u64::from(l2_block_number.0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Address::default(),
        base_fee_per_gas: 0,
        batch_fee_input: Default::default(),
        gas_per_pubdata_limit: 0,
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(Default::default()),
        virtual_blocks: 0,
        gas_limit: 0,
        logs_bloom: Default::default(),
        pubdata_params: PubdataParams::genesis(),
        rolling_txs_hash: Some(H256::zero()),
    };

    conn.blocks_dal()
        .insert_l2_block(&l2_block_header)
        .await
        .unwrap();
    conn.storage_logs_dal()
        .insert_storage_logs(l2_block_number, block_logs)
        .await
        .unwrap();
}

#[allow(clippy::default_trait_access)]
// ^ `BaseSystemContractsHashes::default()` would require a new direct dependency
pub(crate) async fn create_l1_batch(
    conn: &mut Connection<'_, Core>,
    l1_batch_number: L1BatchNumber,
    logs_for_initial_writes: &[StorageLog],
) {
    let header = L1BatchHeader::new(
        l1_batch_number,
        0,
        Default::default(),
        Default::default(),
        SettlementLayer::for_tests(),
    );
    conn.blocks_dal()
        .insert_mock_l1_batch(&header)
        .await
        .unwrap();
    conn.blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_number)
        .await
        .unwrap();

    let mut written_keys: Vec<_> = logs_for_initial_writes.iter().map(|log| log.key).collect();
    written_keys.sort_unstable();
    let written_keys: Vec<_> = written_keys.iter().map(StorageKey::hashed_key).collect();
    conn.storage_logs_dedup_dal()
        .insert_initial_writes(l1_batch_number, &written_keys)
        .await
        .unwrap();
}

pub(crate) fn mock_snapshot_recovery_status() -> SnapshotRecoveryStatus {
    SnapshotRecoveryStatus {
        l1_batch_number: L1BatchNumber(23),
        l1_batch_timestamp: 23,
        l1_batch_root_hash: H256::zero(), // not used
        l2_block_number: L2BlockNumber(42),
        l2_block_timestamp: 42,
        l2_block_hash: H256::zero(), // not used
        protocol_version: ProtocolVersionId::latest(),
        storage_logs_chunks_processed: vec![true; 100],
    }
}

pub(crate) async fn prepare_postgres_for_snapshot_recovery(
    conn: &mut Connection<'_, Core>,
) -> (SnapshotRecoveryStatus, Vec<StorageLog>) {
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    let snapshot_recovery = mock_snapshot_recovery_status();
    conn.snapshot_recovery_dal()
        .insert_initial_recovery_status(&snapshot_recovery)
        .await
        .unwrap();

    let snapshot_storage_logs = gen_storage_logs(100..200);
    conn.storage_logs_dal()
        .insert_storage_logs(snapshot_recovery.l2_block_number, &snapshot_storage_logs)
        .await
        .unwrap();
    let mut written_keys: Vec<_> = snapshot_storage_logs.iter().map(|log| log.key).collect();
    written_keys.sort_unstable();
    let written_keys: Vec<_> = written_keys.iter().map(StorageKey::hashed_key).collect();
    conn.storage_logs_dedup_dal()
        .insert_initial_writes(snapshot_recovery.l1_batch_number, &written_keys)
        .await
        .unwrap();
    (snapshot_recovery, snapshot_storage_logs)
}

pub(crate) async fn prune_storage(
    pool: &ConnectionPool<Core>,
    pruned_l1_batch: L1BatchNumber,
) -> HardPruningStats {
    // Emulate pruning batches in the storage.
    let mut storage = pool.connection().await.unwrap();
    let (_, pruned_l2_block) = storage
        .blocks_dal()
        .get_l2_block_range_of_l1_batch(pruned_l1_batch)
        .await
        .unwrap()
        .expect("L1 batch not present in Postgres");
    let root_hash = H256::zero(); // Doesn't matter for storage recovery

    storage
        .pruning_dal()
        .insert_soft_pruning_log(pruned_l1_batch, pruned_l2_block)
        .await
        .unwrap();
    let pruning_stats = storage
        .pruning_dal()
        .hard_prune_batches_range(pruned_l1_batch, pruned_l2_block)
        .await
        .unwrap();
    assert!(
        pruning_stats.deleted_l1_batches > 0 && pruning_stats.deleted_l2_blocks > 0,
        "{pruning_stats:?}"
    );
    storage
        .pruning_dal()
        .insert_hard_pruning_log(pruned_l1_batch, pruned_l2_block, root_hash)
        .await
        .unwrap();
    pruning_stats
}
