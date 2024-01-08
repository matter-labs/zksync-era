use std::env;

use anyhow::Context as _;
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
        let statement_timeout_sec = env::var("DATABASE_STATEMENT_TIMEOUT_SEC")
            .ok()
            .map(|val| {
                val.parse()
                    .context("failed to parse DATABASE_STATEMENT_TIMEOUT_SEC")
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
    use std::time::Duration;

    use zksync_config::configs::database::MerkleTreeMode;

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DATABASE_STATE_KEEPER_DB_PATH="/db/state_keeper"
            DATABASE_MERKLE_TREE_PATH="/db/tree"
            DATABASE_MERKLE_TREE_MODE=lightweight
            DATABASE_MERKLE_TREE_MULTI_GET_CHUNK_SIZE=250
            DATABASE_MERKLE_TREE_MEMTABLE_CAPACITY_MB=512
            DATABASE_MERKLE_TREE_STALLED_WRITES_TIMEOUT_SEC=60
            DATABASE_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER=50
        "#;
        lock.set_env(config);

        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.state_keeper_db_path, "/db/state_keeper");
        assert_eq!(db_config.merkle_tree.path, "/db/tree");
        assert_eq!(db_config.merkle_tree.mode, MerkleTreeMode::Lightweight);
        assert_eq!(db_config.merkle_tree.multi_get_chunk_size, 250);
        assert_eq!(db_config.merkle_tree.max_l1_batches_per_iter, 50);
        assert_eq!(db_config.merkle_tree.memtable_capacity_mb, 512);
        assert_eq!(db_config.merkle_tree.stalled_writes_timeout_sec, 60);
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
        ]);

        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.state_keeper_db_path, "./db/state_keeper");
        assert_eq!(db_config.merkle_tree.path, "./db/lightweight-new");
        assert_eq!(db_config.merkle_tree.mode, MerkleTreeMode::Full);
        assert_eq!(db_config.merkle_tree.multi_get_chunk_size, 500);
        assert_eq!(db_config.merkle_tree.max_l1_batches_per_iter, 20);
        assert_eq!(db_config.merkle_tree.block_cache_size_mb, 128);
        assert_eq!(db_config.merkle_tree.memtable_capacity_mb, 256);
        assert_eq!(db_config.merkle_tree.stalled_writes_timeout_sec, 30);

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

    #[test]
    fn postgres_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DATABASE_URL=postgres://postgres@localhost/zksync_local
            DATABASE_POOL_SIZE=50
            DATABASE_STATEMENT_TIMEOUT_SEC=300
        "#;
        lock.set_env(config);

        let postgres_config = PostgresConfig::from_env().unwrap();
        assert_eq!(
            postgres_config.master_url().unwrap(),
            "postgres://postgres@localhost/zksync_local"
        );
        assert_eq!(postgres_config.max_connections().unwrap(), 50);
        assert_eq!(
            postgres_config.statement_timeout(),
            Some(Duration::from_secs(300))
        );
    }
}
