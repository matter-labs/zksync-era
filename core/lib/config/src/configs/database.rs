use serde::Deserialize;
use std::env;
use std::time::Duration;

/// Database configuration.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct DBConfig {
    /// Path to the database data directory.
    pub path: String,
    /// Path to the database data directory that serves state cache.
    pub state_keeper_db_path: String,
    /// Path to merkle tree backup directory
    pub merkle_tree_backup_path: String,
    /// Fast ssd path
    pub merkle_tree_fast_ssd_path: String,
    /// Number of backups to keep
    pub backup_count: usize,
    /// Time interval between performing backups
    pub backup_interval_ms: u64,
    /// Maximum number of blocks to be processed by the full tree at a time
    pub max_block_batch: usize,
}

impl Default for DBConfig {
    fn default() -> Self {
        Self {
            path: "./db".to_owned(),
            state_keeper_db_path: "./db/state_keeper".to_owned(),
            merkle_tree_backup_path: "./db/backups".to_owned(),
            merkle_tree_fast_ssd_path: "./db/lightweight".to_owned(),
            backup_count: 5,
            backup_interval_ms: 60_000,
            max_block_batch: 100,
        }
    }
}

impl DBConfig {
    pub fn from_env() -> Self {
        let mut config = DBConfig::default();
        if let Ok(path) = env::var("DATABASE_PATH") {
            config.path = path;
        }
        if let Ok(path) = env::var("DATABASE_STATE_KEEPER_DB_PATH") {
            config.state_keeper_db_path = path;
        }
        if let Ok(path) = env::var("DATABASE_MERKLE_TREE_BACKUP_PATH") {
            config.merkle_tree_backup_path = path;
        }
        if let Ok(path) = env::var("DATABASE_MERKLE_TREE_FAST_SSD_PATH") {
            config.merkle_tree_fast_ssd_path = path;
        }
        if let Ok(Ok(count)) = env::var("DATABASE_BACKUP_COUNT").map(|s| s.parse()) {
            config.backup_count = count;
        }
        if let Ok(Ok(interval)) = env::var("DATABASE_BACKUP_INTERVAL_MS").map(|s| s.parse()) {
            config.backup_interval_ms = interval;
        }
        if let Ok(Ok(size)) = env::var("DATABASE_MAX_BLOCK_BATCH").map(|s| s.parse()) {
            config.max_block_batch = size;
        }
        config
    }

    /// Path to the database data directory.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Path to the database data directory that serves state cache.
    pub fn state_keeper_db_path(&self) -> &str {
        &self.state_keeper_db_path
    }

    /// Path to the merkle tree backup directory.
    pub fn merkle_tree_backup_path(&self) -> &str {
        &self.merkle_tree_backup_path
    }

    pub fn merkle_tree_fast_ssd_path(&self) -> &str {
        &self.merkle_tree_fast_ssd_path
    }

    /// Number of backups to keep
    pub fn backup_count(&self) -> usize {
        self.backup_count
    }

    pub fn backup_interval(&self) -> Duration {
        Duration::from_millis(self.backup_interval_ms)
    }

    pub fn max_block_batch(&self) -> usize {
        self.max_block_batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::set_env;

    fn expected_config() -> DBConfig {
        DBConfig {
            path: "./db".to_owned(),
            state_keeper_db_path: "./db/state_keeper".to_owned(),
            merkle_tree_backup_path: "./db/backups".to_owned(),
            merkle_tree_fast_ssd_path: "./db/lightweight".to_owned(),
            backup_count: 5,
            backup_interval_ms: 60_000,
            max_block_batch: 100,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
DATABASE_PATH="./db"
DATABASE_STATE_KEEPER_DB_PATH="./db/state_keeper"
DATABASE_MERKLE_TREE_BACKUP_PATH="./db/backups"
DATABASE_MERKLE_TREE_FAST_SSD_PATH="./db/lightweight"
DATABASE_BACKUP_COUNT=5
DATABASE_BACKUP_INTERVAL_MS=60000
DATABASE_MAX_BLOCK_BATCH=100
        "#;
        set_env(config);

        let actual = DBConfig::from_env();
        assert_eq!(actual, expected_config());
    }

    /// Checks the correctness of the config helper methods.
    #[test]
    fn methods() {
        let config = expected_config();

        assert_eq!(config.path(), &config.path);
        assert_eq!(config.state_keeper_db_path(), &config.state_keeper_db_path);
        assert_eq!(
            config.merkle_tree_backup_path(),
            &config.merkle_tree_backup_path
        );
        assert_eq!(
            config.merkle_tree_fast_ssd_path(),
            &config.merkle_tree_fast_ssd_path
        );
        assert_eq!(config.backup_count(), config.backup_count);
        assert_eq!(config.backup_interval().as_secs(), 60);
    }
}
