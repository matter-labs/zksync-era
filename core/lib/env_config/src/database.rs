use anyhow::Context as _;
use std::env;
use zksync_config::{DBConfig, PostgresConfig};

use crate::{envy_load, FromEnv};

impl FromEnv for DBConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            merkle_tree: envy_load("database_merkle_tree", "DATABASE_MERKLE_TREE_")?,
            ..envy_load("database", "DATABASE_")?
        })
    }
}

impl FromEnv for PostgresConfig {
    fn from_env() -> anyhow::Result<Self> {
        let master_url = env::var("DATABASE_URL").ok();
        let replica_url = env::var("DATABASE_REPLICA_URL")
            .ok()
            .or_else(|| master_url.clone());
        let prover_url = env::var("DATABASE_PROVER_URL")
            .ok()
            .or_else(|| master_url.clone());
        let max_connections = env::var("DATABASE_POOL_SIZE")
            .ok()
            .map(|val| val.parse().context("failed to parse DATABASE_POOL_SIZE"))
            .transpose()?;
        let statement_timeout_sec = env::var("DATABASE_STATEMENT_TIMEOUT")
            .ok()
            .map(|val| {
                val.parse()
                    .context("failed to parse DATABASE_STATEMENT_TIMEOUT")
            })
            .transpose()?;

        Ok(Self {
            master_url,
            replica_url,
            prover_url,
            max_connections,
            statement_timeout_sec,
        })
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::database::MerkleTreeMode;

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DATABASE_STATE_KEEPER_DB_PATH="/db/state_keeper"
            DATABASE_MERKLE_TREE_BACKUP_PATH="/db/backups"
            DATABASE_MERKLE_TREE_PATH="/db/tree"
            DATABASE_MERKLE_TREE_MODE=lightweight
            DATABASE_MERKLE_TREE_MULTI_GET_CHUNK_SIZE=250
            DATABASE_MERKLE_TREE_MEMTABLE_CAPACITY_MB=512
            DATABASE_MERKLE_TREE_STALLED_WRITES_TIMEOUT_SEC=60
            DATABASE_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER=50
            DATABASE_BACKUP_COUNT=5
            DATABASE_BACKUP_INTERVAL_MS=60000
        "#;
        lock.set_env(config);

        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.state_keeper_db_path, "/db/state_keeper");
        assert_eq!(db_config.merkle_tree.path, "/db/tree");
        assert_eq!(db_config.merkle_tree.backup_path, "/db/backups");
        assert_eq!(db_config.merkle_tree.mode, MerkleTreeMode::Lightweight);
        assert_eq!(db_config.merkle_tree.multi_get_chunk_size, 250);
        assert_eq!(db_config.merkle_tree.max_l1_batches_per_iter, 50);
        assert_eq!(db_config.merkle_tree.memtable_capacity_mb, 512);
        assert_eq!(db_config.merkle_tree.stalled_writes_timeout_sec, 60);
        assert_eq!(db_config.backup_count, 5);
        assert_eq!(db_config.backup_interval().as_secs(), 60);
    }

    #[test]
    fn from_empty_env() {
        let mut lock = MUTEX.lock();
        lock.remove_env(&[
            "DATABASE_STATE_KEEPER_DB_PATH",
            "DATABASE_MERKLE_TREE_BACKUP_PATH",
            "DATABASE_MERKLE_TREE_PATH",
            "DATABASE_MERKLE_TREE_MODE",
            "DATABASE_MERKLE_TREE_MULTI_GET_CHUNK_SIZE",
            "DATABASE_MERKLE_TREE_BLOCK_CACHE_SIZE_MB",
            "DATABASE_MERKLE_TREE_MEMTABLE_CAPACITY_MB",
            "DATABASE_MERKLE_TREE_STALLED_WRITES_TIMEOUT_SEC",
            "DATABASE_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER",
            "DATABASE_BACKUP_COUNT",
            "DATABASE_BACKUP_INTERVAL_MS",
        ]);

        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.state_keeper_db_path, "./db/state_keeper");
        assert_eq!(db_config.merkle_tree.path, "./db/lightweight-new");
        assert_eq!(db_config.merkle_tree.backup_path, "./db/backups");
        assert_eq!(db_config.merkle_tree.mode, MerkleTreeMode::Full);
        assert_eq!(db_config.merkle_tree.multi_get_chunk_size, 500);
        assert_eq!(db_config.merkle_tree.max_l1_batches_per_iter, 20);
        assert_eq!(db_config.merkle_tree.block_cache_size_mb, 128);
        assert_eq!(db_config.merkle_tree.memtable_capacity_mb, 256);
        assert_eq!(db_config.merkle_tree.stalled_writes_timeout_sec, 30);
        assert_eq!(db_config.backup_count, 5);
        assert_eq!(db_config.backup_interval().as_secs(), 60);

        // Check that new env variable for Merkle tree path is supported
        lock.set_env("DATABASE_MERKLE_TREE_PATH=/db/tree/main");
        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.merkle_tree.path, "/db/tree/main");

        lock.set_env("DATABASE_MERKLE_TREE_MULTI_GET_CHUNK_SIZE=200");
        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.merkle_tree.multi_get_chunk_size, 200);

        lock.set_env("DATABASE_MERKLE_TREE_BLOCK_CACHE_SIZE_MB=256");
        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.merkle_tree.block_cache_size_mb, 256);

        lock.set_env("DATABASE_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER=50");
        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.merkle_tree.max_l1_batches_per_iter, 50);
    }
}
