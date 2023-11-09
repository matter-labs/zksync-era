use clap::Parser;

use zksync_config::PostgresConfig;
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_types::MiniblockNumber;

const MIGRATED_TABLE: &str = "storage_logs";
const NOT_MIGRATED_TABLE: &str = "storage_logs_backup";

#[derive(Debug, Parser)]
#[command(
    author = "Matter Labs",
    about = "Consistency checker for the migration"
)]
struct Cli {
    /// Miniblock number to start check from.
    #[arg(long)]
    from_miniblock: u32,
    /// Miniblock number to check up to.
    #[arg(long)]
    to_miniblock: u32,
}

#[tokio::main]
async fn main() {
    let config = PostgresConfig::from_env().unwrap();
    let opt = Cli::parse();
    let pool = ConnectionPool::singleton(config.replica_url().unwrap())
        .build()
        .await
        .unwrap();
    let mut connection = pool.access_storage().await.unwrap();

    println!(
        "Consistency check started for miniblock range {}..={}",
        opt.from_miniblock, opt.to_miniblock
    );

    for miniblock_number in opt.from_miniblock..=opt.to_miniblock {
        let miniblock_number = MiniblockNumber(miniblock_number);
        // Load all storage logs of miniblock.
        let storage_logs = connection
            .storage_logs_dal()
            .get_miniblock_storage_logs_from_table(miniblock_number, NOT_MIGRATED_TABLE)
            .await;

        for (hashed_key, _, _) in storage_logs {
            let value_before_migration = connection
                .storage_logs_dal()
                .get_storage_value_from_table(hashed_key, miniblock_number, NOT_MIGRATED_TABLE)
                .await;
            let value_after_migration = connection
                .storage_logs_dal()
                .get_storage_value_from_table(hashed_key, miniblock_number, MIGRATED_TABLE)
                .await;
            assert_eq!(
                value_before_migration, value_after_migration,
                "Found divergency for hashed_key = {hashed_key:?}, miniblock {miniblock_number}"
            );
        }

        println!("Processed miniblock {miniblock_number}");
    }

    println!("Finished");
}
