use std::{fs, io::BufWriter, path::PathBuf, str::FromStr};

use clap::Parser;
use zksync_config::GenesisConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{custom_genesis_export_dal::GenesisState, ConnectionPool, Core, CoreDal};
use zksync_node_genesis::{make_genesis_batch_params, utils::get_deduped_log_queries};
use zksync_types::{url::SensitiveUrl, StorageLog};

#[derive(Debug, Parser)]
#[command(name = "Custom genesis export tool", author = "Matter Labs")]
struct Args {
    /// PostgreSQL connection string for the database to export.
    #[arg(short, long)]
    database_url: Option<String>,

    /// Output file path.
    #[arg(short, long, default_value = "genesis_export.bin")]
    output_path: PathBuf,

    /// Path to the genesis.yaml
    #[arg(short, long)]
    genesis_config_path: PathBuf,
}

/// The `custom_genesis_export` tool allows exporting storage logs and factory dependencies
/// from the ZKSync PostgreSQL database in a way that they can be used as a custom genesis state for a new chain.
///
/// Inputs:
///     * `database_url` - URL to the PostgreSQL database.
///     * `output` - Path to the output file.
///     * `genesis_config_path` - Path to the `genesis.yaml` configuration file, which will be used to set up a new chain (located in the `file_based` directory).
///
/// Given the inputs above, `custom_genesis_export` will perform the following:
///     * Read storage logs, and factory dependencies; filter out those related to the system context,
///       and save the remaining data to the output file.
///     * Calculate the new `genesis_root_hash`, `rollup_last_leaf_index`, and `genesis_commitment`, then update these
///       in-place in the provided `genesis.yaml`. Additionally, the tool will add a `custom_genesis_state_path` property
///       pointing to the genesis export.
///
/// Note: To calculate the new genesis parameters, the current implementation requires loading all storage logs
/// into RAM. This is necessary due to the specific sorting and filtering that need to be applied.
/// For larger states, keep this in mind and ensure you have a machine with sufficient RAM.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut out = BufWriter::new(fs::File::create(&args.output_path)?);

    println!(
        "Export file: {}",
        args.output_path.canonicalize()?.display(),
    );

    println!("Connecting to source database...");

    let db_url = args.database_url.or_else(|| std::env::var("DATABASE_URL").ok()).expect("Specify the database connection string in either a CLI argument or in the DATABASE_URL environment variable.");
    // we need only 1 DB connection at most for data export
    let connection_pool_builder =
        ConnectionPool::<Core>::builder(SensitiveUrl::from_str(db_url.as_str())?, 1);
    let connection_pool = connection_pool_builder.build().await?;

    let mut storage = connection_pool.connection().await?;
    let mut transaction = storage.start_transaction().await?;

    println!("Connected to source database.");

    let storage_logs = transaction
        .custom_genesis_export_dal()
        .get_storage_logs()
        .await?;
    let factory_deps = transaction
        .custom_genesis_export_dal()
        .get_factory_deps()
        .await?;

    transaction.commit().await?;

    println!(
        "Loaded {} storage logs {} factory deps from source database.",
        storage_logs.len(),
        factory_deps.len()
    );

    let storage_logs_for_genesis: Vec<StorageLog> =
        storage_logs.iter().map(StorageLog::from).collect();

    bincode::serialize_into(
        &mut out,
        &GenesisState {
            storage_logs,
            factory_deps,
        },
    )?;

    println!(
        "Saved genesis state into the file {}.",
        args.output_path.display()
    );
    println!("Calculating new genesis parameters");

    let genesis_config_path = args.genesis_config_path.clone();
    let mut genesis_config =
        tokio::task::spawn_blocking(move || GenesisConfig::read(&genesis_config_path)).await??;

    let base_system_contract_hashes = BaseSystemContractsHashes {
        bootloader: genesis_config
            .bootloader_hash
            .ok_or(anyhow::anyhow!("No bootloader_hash specified"))?,
        default_aa: genesis_config
            .default_aa_hash
            .ok_or(anyhow::anyhow!("No default_aa_hash specified"))?,
        evm_emulator: genesis_config.evm_emulator_hash,
    };

    let (genesis_batch_params, _) = make_genesis_batch_params(
        get_deduped_log_queries(&storage_logs_for_genesis),
        base_system_contract_hashes,
        genesis_config
            .protocol_version
            .ok_or(anyhow::anyhow!("No bootloader_hash specified"))?
            .minor,
    );

    genesis_config.genesis_root_hash = Some(genesis_batch_params.root_hash);
    genesis_config.rollup_last_leaf_index = Some(genesis_batch_params.rollup_last_leaf_index);
    genesis_config.genesis_commitment = Some(genesis_batch_params.commitment);
    genesis_config.custom_genesis_state_path =
        args.output_path.canonicalize()?.to_str().map(String::from);

    let genesis_config_path = args.genesis_config_path;
    tokio::task::spawn_blocking(move || genesis_config.write(&genesis_config_path)).await??;

    println!("Done.");

    Ok(())
}
