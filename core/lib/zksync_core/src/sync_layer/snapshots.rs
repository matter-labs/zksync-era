use crate::sync_layer::MainNodeClient;
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;
use std::time::Instant;
use zksync_dal::StorageProcessor;
use zksync_merkle_tree::recovery::{MerkleTreeRecovery, RecoveryEntry};
use zksync_merkle_tree::{PruneDatabase, RocksDBWrapper};
use zksync_object_store::ObjectStore;
use zksync_storage::RocksDB;
use zksync_types::block::{BlockGasCount, MiniblockHeader};
use zksync_types::commitment::L1BatchWithMetadata;
use zksync_types::snapshots::{
    AppliedSnapshotStatus, FactoryDependency, SingleStorageLogSnapshot, SnapshotBasicMetadata,
    SnapshotChunk, SnapshotStorageKey,
};
use zksync_types::{
    MiniblockNumber, ProtocolVersionId, StorageKey, StorageLog, StorageLogKind, H256,
};

async fn sync_protocol_version(
    storage: &mut StorageProcessor<'_>,
    client: &dyn MainNodeClient,
    id: ProtocolVersionId,
) {
    let protocol_version = client
        .fetch_protocol_version(id)
        .await
        .expect("Failed to fetch protocol version from the main node");
    storage
        .protocol_versions_dal()
        .save_protocol_version(
            protocol_version.version_id.try_into().unwrap(),
            protocol_version.timestamp,
            protocol_version.verification_keys_hashes,
            protocol_version.base_system_contracts,
            // Verifier is not used in the external node, so we can pass an empty
            Default::default(),
            protocol_version.l2_system_upgrade_tx_hash,
        )
        .await;
}

async fn sync_miniblock_header(
    storage: &mut StorageProcessor<'_>,
    client: &dyn MainNodeClient,
    miniblock_number: MiniblockNumber,
) {
    let sync_block = client
        .fetch_l2_block(miniblock_number, false)
        .await
        .expect("Failed to fetch block from the main node")
        .expect("Block must exist");
    sync_protocol_version(storage, client, sync_block.protocol_version).await;
    storage
        .blocks_dal()
        .insert_miniblock(&MiniblockHeader {
            number: sync_block.number,
            timestamp: sync_block.timestamp,
            hash: sync_block.hash.unwrap(),
            l1_tx_count: 0,
            l2_tx_count: 0,
            base_fee_per_gas: 0,
            l1_gas_price: sync_block.l1_gas_price,
            l2_fair_gas_price: sync_block.l2_fair_gas_price,
            base_system_contracts_hashes: sync_block.base_system_contracts_hashes,
            protocol_version: Some(sync_block.protocol_version),
            virtual_blocks: sync_block.virtual_blocks.unwrap(),
        })
        .await
        .unwrap();
    storage
        .blocks_dal()
        .update_hashes(&[(sync_block.number, sync_block.hash.unwrap())])
        .await
        .unwrap();
    tracing::info!("Fetched miniblock {} from main node", sync_block.number)
}

async fn sync_l1_batch_metadata(
    storage: &mut StorageProcessor<'_>,
    l1_batch_header_with_metadata: &L1BatchWithMetadata,
) {
    let l1_batch_header = &l1_batch_header_with_metadata.header;
    let l1_batch_metadata = &l1_batch_header_with_metadata.metadata;

    storage
        .blocks_dal()
        .insert_l1_batch(l1_batch_header, &[], BlockGasCount::default(), &[], &[])
        .await
        .unwrap();
    storage
        .blocks_dal()
        .save_l1_batch_metadata(l1_batch_header.number, l1_batch_metadata, H256::zero())
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(l1_batch_header.number)
        .await
        .unwrap();
}

async fn sync_storage_logs_from_snapshot_chunk(
    storage: &mut StorageProcessor<'_>,
    snapshot_metadata: &SnapshotBasicMetadata,
    storage_logs: &[SingleStorageLogSnapshot],
) {
    tracing::info!("Loading {} storage logs into postgres", storage_logs.len());
    let storage_logs_keys: Vec<StorageKey> = storage_logs.iter().map(|log| log.key).collect();
    storage
        .storage_logs_dedup_dal()
        .insert_initial_writes(snapshot_metadata.l1_batch_number, &storage_logs_keys)
        .await;

    let transformed_logs = storage_logs
        .iter()
        .map(|log| StorageLog {
            kind: StorageLogKind::Write,
            key: log.key,
            value: log.value,
        })
        .collect();
    storage
        .storage_logs_dal()
        .append_storage_logs(
            snapshot_metadata.miniblock_number,
            &[(H256::zero(), transformed_logs)],
        )
        .await;
}

async fn sync_tree_from_snapshot_chunk(
    recovery: &mut MerkleTreeRecovery<'_, &mut dyn PruneDatabase>,
    storage_logs: &[SingleStorageLogSnapshot],
) {
    let logs_for_merkle_tree = storage_logs
        .iter()
        .map(|log| RecoveryEntry {
            key: log.key.hashed_key_u256(),
            value: log.value,
            leaf_index: log.enumeration_index,
        })
        .collect();

    recovery.extend(logs_for_merkle_tree);
}

async fn fetch_storage_logs_chunk(
    blob_store: &dyn ObjectStore,
    metadata: &SnapshotBasicMetadata,
    chunk_id: u64,
) -> SnapshotChunk {
    tracing::info!("Fetching snapshot chunk {chunk_id}");
    blob_store
        .get(SnapshotStorageKey {
            l1_batch_number: metadata.l1_batch_number,
            chunk_id,
        })
        .await
        .unwrap()
}

async fn sync_factory_deps_from_snapshot_chunk(
    storage: &mut StorageProcessor<'_>,
    metadata: &SnapshotBasicMetadata,
    factory_deps: Vec<FactoryDependency>,
) {
    if !factory_deps.is_empty() {
        let all_deps_hashmap: HashMap<H256, Vec<u8>> = factory_deps
            .into_iter()
            .map(|dep| (dep.bytecode_hash, dep.bytecode))
            .collect();
        storage
            .storage_dal()
            .insert_factory_deps(metadata.miniblock_number, &all_deps_hashmap)
            .await;
    }
}

pub async fn load_snapshot_if_needed(
    storage: &mut StorageProcessor<'_>,
    client: &dyn MainNodeClient,
    blob_store: &dyn ObjectStore,
    merkle_tree_db_path: &String,
) -> anyhow::Result<()> {
    let mut applied_snapshot_status = storage
        .applied_snapshot_status_dal()
        .get_applied_snapshot_status()
        .await
        .unwrap();

    if !storage.blocks_dal().is_genesis_needed().await.unwrap() && applied_snapshot_status.is_none()
    {
        tracing::info!("This node has already been initialized without a snapshot, skipping");
        return Ok(());
    }

    if applied_snapshot_status.is_some() && applied_snapshot_status.as_ref().unwrap().is_finished {
        tracing::info!("This node has already been initialized from a snapshot, skipping!");
        return Ok(());
    }

    let snapshot_response = client.fetch_newest_snapshot().await.unwrap();
    if snapshot_response.is_none() {
        tracing::info!(
            "Main node does not have any ready snapshots, skipping initialization from snapshot!"
        );
        return Ok(());
    }
    let snapshot = snapshot_response.unwrap();
    tracing::info!("Found snapshot with data up to l1_batch {}, created at {}, storage_logs are divided into {} chunk(s)", snapshot.metadata.l1_batch_number, snapshot.metadata.generated_at, snapshot.storage_logs_files.len());

    if applied_snapshot_status.is_none() {
        applied_snapshot_status = Some(AppliedSnapshotStatus {
            l1_batch_number: snapshot.metadata.l1_batch_number,
            miniblock_number: snapshot.metadata.miniblock_number,
            is_finished: false,
            last_finished_chunk_id: None,
        });
    };
    let mut applied_snapshot_status = applied_snapshot_status.unwrap();

    storage
        .applied_snapshot_status_dal()
        .set_applied_snapshot_status(&applied_snapshot_status)
        .await
        .unwrap();

    let metadata = snapshot.metadata;

    let recovered_version = metadata.l1_batch_number.0 as u64;
    let db = RocksDB::new(Path::new(&merkle_tree_db_path));
    let db: &mut dyn PruneDatabase = &mut (RocksDBWrapper::from(db));
    let mut recovery = MerkleTreeRecovery::new(db, recovered_version);

    sync_miniblock_header(storage, client, metadata.miniblock_number).await;

    sync_l1_batch_metadata(storage, &snapshot.last_l1_batch_with_metadata).await;

    applied_snapshot_status.is_finished = true;
    storage
        .applied_snapshot_status_dal()
        .set_applied_snapshot_status(&applied_snapshot_status)
        .await
        .unwrap();

    for (chunk_id, _) in snapshot.storage_logs_files.iter().enumerate() {
        if applied_snapshot_status.last_finished_chunk_id.is_some()
            && chunk_id as u64 > applied_snapshot_status.last_finished_chunk_id.unwrap()
        {
            tracing::info!(
                "Skipping processing chunk {}, file already processed",
                chunk_id
            );
        }

        let storage_snapshot_chunk =
            fetch_storage_logs_chunk(blob_store, &metadata, chunk_id as u64).await;

        let factory_deps = storage_snapshot_chunk.factory_deps;
        sync_factory_deps_from_snapshot_chunk(storage, &metadata, factory_deps).await;

        let storage_logs = &storage_snapshot_chunk.storage_logs;
        sync_storage_logs_from_snapshot_chunk(storage, &metadata, storage_logs).await;
        sync_tree_from_snapshot_chunk(&mut recovery, storage_logs).await;

        applied_snapshot_status.last_finished_chunk_id = Some(chunk_id as u64);
        storage
            .applied_snapshot_status_dal()
            .set_applied_snapshot_status(&applied_snapshot_status)
            .await
            .unwrap();
    }

    tracing::info!("Processing chunks finished, finalizing merkle tree");

    let tree = recovery.finalize();
    let started_at = Instant::now();
    tree.verify_consistency(recovered_version).unwrap();
    tracing::info!("Verified consistency in {:?}", started_at.elapsed());

    Ok(())
}
