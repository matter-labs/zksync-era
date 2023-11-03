use anyhow::Context as _;
use clap::{Parser, Subcommand};

use zksync_config::DBConfig;
use zksync_env_config::FromEnv;
use zksync_storage::rocksdb::{
    backup::{BackupEngine, BackupEngineOptions, RestoreOptions},
    Env, Error, Options, DB,
};

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "RocksDB management utility", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Creates new backup of running RocksDB instance.
    #[command(name = "backup")]
    Backup,
    /// Restores RocksDB from backup.
    #[command(name = "restore-from-backup")]
    Restore,
}

fn create_backup(config: &DBConfig) -> Result<(), Error> {
    let mut engine = BackupEngine::open(
        &BackupEngineOptions::new(&config.merkle_tree.backup_path)?,
        &Env::new()?,
    )?;
    let db_dir = &config.merkle_tree.path;
    let db = DB::open_for_read_only(&Options::default(), db_dir, false)?;
    engine.create_new_backup(&db)?;
    engine.purge_old_backups(config.backup_count)
}

fn restore_from_latest_backup(config: &DBConfig) -> Result<(), Error> {
    let mut engine = BackupEngine::open(
        &BackupEngineOptions::new(&config.merkle_tree.backup_path)?,
        &Env::new()?,
    )?;
    let db_dir = &config.merkle_tree.path;
    engine.restore_from_latest_backup(db_dir, db_dir, &RestoreOptions::default())
}

fn main() -> anyhow::Result<()> {
    let db_config = DBConfig::from_env().context("DBConfig::from_env()")?;
    match Cli::parse().command {
        Command::Backup => create_backup(&db_config).context("create_backup"),
        Command::Restore => {
            restore_from_latest_backup(&db_config).context("restore_from_latest_backup")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn backup_restore_workflow() {
        let backup_dir = TempDir::new().expect("failed to get temporary directory for RocksDB");
        let temp_dir = TempDir::new().expect("failed to get temporary directory for RocksDB");
        let mut db_config = DBConfig::from_env().unwrap();
        db_config.merkle_tree.path = temp_dir.path().to_str().unwrap().to_string();
        db_config.merkle_tree.backup_path = backup_dir.path().to_str().unwrap().to_string();
        let db_dir = &db_config.merkle_tree.path;

        let mut options = Options::default();
        options.create_if_missing(true);
        let db = DB::open(&options, db_dir).unwrap();
        db.put(b"key", b"value").expect("failed to write to db");

        create_backup(&db_config).expect("failed to create backup");
        // Drop original database
        drop((db, temp_dir));

        restore_from_latest_backup(&db_config).expect("failed to restore from backup");
        let db = DB::open(&Options::default(), db_dir).unwrap();
        assert_eq!(db.get(b"key").unwrap().unwrap(), b"value");
    }
}
