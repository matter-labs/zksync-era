use clap::{Parser, Subcommand};
use zksync_config::DBConfig;
use zksync_storage::rocksdb::backup::{BackupEngine, BackupEngineOptions, RestoreOptions};
use zksync_storage::rocksdb::{Error, Options, DB};

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
        &BackupEngineOptions::default(),
        config.merkle_tree_backup_path(),
    )?;
    let db_dir = &config.new_merkle_tree_ssd_path;
    let db = DB::open_for_read_only(&Options::default(), db_dir, false)?;
    engine.create_new_backup(&db)?;
    engine.purge_old_backups(config.backup_count())
}

fn restore_from_latest_backup(config: &DBConfig) -> Result<(), Error> {
    let mut engine = BackupEngine::open(
        &BackupEngineOptions::default(),
        config.merkle_tree_backup_path(),
    )?;
    let db_dir = &config.new_merkle_tree_ssd_path;
    engine.restore_from_latest_backup(db_dir, db_dir, &RestoreOptions::default())
}

fn main() {
    let db_config = DBConfig::from_env();
    match Cli::parse().command {
        Command::Backup => create_backup(&db_config).unwrap(),
        Command::Restore => restore_from_latest_backup(&db_config).unwrap(),
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
        let db_config = DBConfig {
            new_merkle_tree_ssd_path: temp_dir.path().to_str().unwrap().to_string(),
            merkle_tree_backup_path: backup_dir.path().to_str().unwrap().to_string(),
            ..Default::default()
        };
        let db_dir = &db_config.new_merkle_tree_ssd_path;

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
