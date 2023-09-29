use zksync_config::DBConfig;
use zksync_dal::{connection::DbVariant, ConnectionPool};
use zksync_merkle_tree::domain::ZkSyncTree;
use zksync_storage::RocksDB;
use zksync_types::{L1BatchNumber, H256, U256};

pub fn u256_to_h256_rev(num: U256) -> H256 {
    let mut bytes = [0u8; 32];
    num.to_little_endian(&mut bytes);
    H256::from_slice(&bytes)
}

#[tokio::main]
async fn main() {
    vlog::init();
    let db_path = DBConfig::from_env().merkle_tree.path;
    vlog::info!("Verifying consistency of slot indices");

    let pool = ConnectionPool::singleton(DbVariant::Replica).build().await;
    let mut storage = pool.access_storage().await;

    let db = RocksDB::new(db_path, true);
    let tree = ZkSyncTree::new_lightweight(db);

    let next_number = tree.next_l1_batch_number();
    if next_number == L1BatchNumber(0) {
        vlog::info!("Merkle tree is empty, skipping");
        return;
    }
    let tree_l1_batch_number = next_number - 1;
    let pg_l1_batch_number = storage.blocks_dal().get_sealed_l1_batch_number().await;

    let check_up_to_l1_batch_number = tree_l1_batch_number.min(pg_l1_batch_number);

    for l1_batch_number in 0..=check_up_to_l1_batch_number.0 {
        vlog::info!("Checking indices for L1 batch {l1_batch_number}");
        let pg_keys: Vec<_> = storage
            .storage_logs_dedup_dal()
            .initial_writes_for_batch(l1_batch_number.into())
            .await
            .into_iter()
            .map(|(key, index)| {
                (
                    key,
                    index.expect("Missing index in database, migration should be run beforehand"),
                )
            })
            .collect();
        let keys_u256: Vec<_> = pg_keys
            .iter()
            .map(|(key, _)| U256::from_little_endian(key.as_bytes()))
            .collect();

        let tree_keys: Vec<_> = tree
            .read_leaves(l1_batch_number.into(), &keys_u256)
            .into_iter()
            .zip(keys_u256)
            .map(|(leaf_data, key)| (u256_to_h256_rev(key), leaf_data.unwrap().leaf_index))
            .collect();
        assert_eq!(pg_keys, tree_keys);

        vlog::info!("Indices are consistent for L1 batch {l1_batch_number}");
    }
}
