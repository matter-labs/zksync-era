extern crate core;

use std::{fs, fs::File, io::BufWriter, path::PathBuf, str::FromStr, time::Instant};

use clap::Parser;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_core_leftovers::temp_config_store::read_yaml_repr;
use zksync_dal::{
    custom_genesis_export_dal::{FactoryDepRow, GenesisState, StorageLogRow},
    ConnectionPool, Core, CoreDal,
};
use zksync_node_genesis::{make_genesis_batch_params, utils::get_deduped_log_queries};
use zksync_protobuf_config::encode_yaml_repr;
use zksync_types::{url::SensitiveUrl, zk_evm_types::LogQuery, StorageLog};

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
    let output_path = args.output_path;
    let db_url = args.database_url.or_else(|| std::env::var("DATABASE_URL").ok()).expect("Specify the database connection string in either a CLI argument or in the DATABASE_URL environment variable.");

    let mut start = Instant::now();
    // we need only 1 DB connection at most for data export
    let connection_pool_builder =
        ConnectionPool::<Core>::builder(SensitiveUrl::from_str(db_url.as_str())?, 1);
    let connection_pool = connection_pool_builder.build().await?;

    println!("Connected to the database in {:?}.", start.elapsed());

    start = Instant::now();
    let (storage_logs, factory_deps) =
        read_storage_logs_and_factory_deps_from_db(connection_pool).await?;

    println!(
        "Loaded {} storage logs and {} factory deps from the database in {:?}.",
        storage_logs.len(),
        factory_deps.len(),
        start.elapsed(),
    );

    let storage_logs_for_genesis: Vec<StorageLog> =
        storage_logs.iter().map(StorageLog::from).collect();
    start = Instant::now();
    persist_custom_genesis_file(output_path.clone(), storage_logs, factory_deps).await?;

    println!("Saved genesis state in {:?}.", start.elapsed());

    start = Instant::now();
    let mut genesis_config = read_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(
        &args.genesis_config_path,
    )?;

    let base_system_contract_hashes = BaseSystemContractsHashes {
        bootloader: genesis_config
            .bootloader_hash
            .ok_or(anyhow::anyhow!("No bootloader_hash specified"))?,
        default_aa: genesis_config
            .default_aa_hash
            .ok_or(anyhow::anyhow!("No default_aa_hash specified"))?,
        evm_emulator: genesis_config.evm_emulator_hash,
    };

    let deduped_log_queries = do_get_deduped_log_queries(storage_logs_for_genesis);
    let (genesis_batch_params, _) = make_genesis_batch_params(
        deduped_log_queries,
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
        output_path.canonicalize()?.to_str().map(String::from);

    let bytes =
        encode_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(&genesis_config)?;
    fs::write(&args.genesis_config_path, &bytes)?;

    println!("Calculated custom genesis params in {:?}.", start.elapsed());
    Ok(())
}

fn do_get_deduped_log_queries(storage_logs_for_genesis: Vec<StorageLog>) -> Vec<LogQuery> {
    get_deduped_log_queries(&storage_logs_for_genesis)
}

async fn read_storage_logs_and_factory_deps_from_db(
    connection_pool: ConnectionPool<Core>,
) -> anyhow::Result<(Vec<StorageLogRow>, Vec<FactoryDepRow>)> {
    let mut storage = connection_pool.connection().await?;
    let mut transaction = storage.start_transaction().await?;

    let storage_logs = transaction
        .custom_genesis_export_dal()
        .get_storage_logs()
        .await?;
    let factory_deps = transaction
        .custom_genesis_export_dal()
        .get_factory_deps()
        .await?;
    transaction.commit().await?;
    Ok((storage_logs, factory_deps))
}

async fn persist_custom_genesis_file(
    output_path: PathBuf,
    storage_logs: Vec<StorageLogRow>,
    factory_deps: Vec<FactoryDepRow>,
) -> anyhow::Result<()> {
    let mut out = BufWriter::new(File::create(output_path)?);
    bincode::serialize_into(
        &mut out,
        &GenesisState {
            storage_logs,
            factory_deps,
        },
    )?;
    Ok(())
}
