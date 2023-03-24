use zksync_config::ZkSyncConfig;
use zksync_merkle_tree::ZkSyncTree;
use zksync_storage::db::Database;
use zksync_storage::RocksDB;

fn main() {
    let _sentry_guard = vlog::init();
    let config = ZkSyncConfig::from_env();
    let db = RocksDB::new(Database::MerkleTree, config.db.path(), true);
    let tree = ZkSyncTree::new(db);
    tree.verify_consistency();
}
