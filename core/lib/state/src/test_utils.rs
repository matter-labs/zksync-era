//! Shared utils for unit tests.

use zksync_dal::StorageProcessor;
use zksync_types::{
    block::{BlockGasCount, L1BatchHeader, MiniblockHeader},
    AccountTreeId, Address, L1BatchNumber, MiniblockNumber, ProtocolVersion, StorageKey,
    StorageLog, H256,
};

use std::ops;

pub(crate) async fn prepare_postgres(conn: &mut StorageProcessor<'_>) {
    if conn.blocks_dal().is_genesis_needed().await.unwrap() {
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        // The created genesis block is likely to be invalid, but since it's not committed,
        // we don't really care.
        let genesis_storage_logs = gen_storage_logs(0..20);
        create_miniblock(conn, MiniblockNumber(0), genesis_storage_logs.clone()).await;
        create_l1_batch(conn, L1BatchNumber(0), &genesis_storage_logs).await;
    }

    conn.storage_logs_dal()
        .rollback_storage_logs(MiniblockNumber(0))
        .await;
    conn.blocks_dal()
        .delete_miniblocks(MiniblockNumber(0))
        .await
        .unwrap();
    conn.blocks_dal()
        .delete_l1_batches(L1BatchNumber(0))
        .await
        .unwrap();
}

pub(crate) fn gen_storage_logs(indices: ops::Range<u64>) -> Vec<StorageLog> {
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
pub(crate) async fn create_miniblock(
    conn: &mut StorageProcessor<'_>,
    miniblock_number: MiniblockNumber,
    block_logs: Vec<StorageLog>,
) {
    let miniblock_header = MiniblockHeader {
        number: miniblock_number,
        timestamp: 0,
        hash: H256::from_low_u64_be(u64::from(miniblock_number.0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: 0,
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(Default::default()),
        virtual_blocks: 0,
    };

    conn.blocks_dal()
        .insert_miniblock(&miniblock_header)
        .await
        .unwrap();
    conn.storage_logs_dal()
        .insert_storage_logs(miniblock_number, &[(H256::zero(), block_logs)])
        .await;
}

#[allow(clippy::default_trait_access)]
// ^ `BaseSystemContractsHashes::default()` would require a new direct dependency
pub(crate) async fn create_l1_batch(
    conn: &mut StorageProcessor<'_>,
    l1_batch_number: L1BatchNumber,
    logs_for_initial_writes: &[StorageLog],
) {
    let mut header = L1BatchHeader::new(
        l1_batch_number,
        0,
        Address::default(),
        Default::default(),
        Default::default(),
    );
    header.is_finished = true;
    conn.blocks_dal()
        .insert_l1_batch(&header, &[], BlockGasCount::default(), &[], &[])
        .await
        .unwrap();
    conn.blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(l1_batch_number)
        .await
        .unwrap();

    let mut written_keys: Vec<_> = logs_for_initial_writes.iter().map(|log| log.key).collect();
    written_keys.sort_unstable();
    conn.storage_logs_dedup_dal()
        .insert_initial_writes(l1_batch_number, &written_keys)
        .await;
}
