use std::{
    fs,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

use clap::Parser;
use futures::TryStreamExt;
use sqlx::{prelude::*, Connection, PgConnection};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_core_leftovers::temp_config_store::read_yaml_repr;
use zksync_node_genesis::make_genesis_batch_params;
use zksync_protobuf_config::encode_yaml_repr;
use zksync_types::{AccountTreeId, StorageKey, StorageLog, H160, H256};

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
#[derive(FromRow)]
struct InitialWriteRow {
    hashed_key: [u8; 32],
    index: i64,
}
#[derive(FromRow)]
struct StorageLogRow {
    address: [u8; 20],
    key: [u8; 32],
    value: [u8; 32],
}
#[derive(FromRow)]
struct FactoryDepRow {
    bytecode_hash: [u8; 32],
    bytecode: Vec<u8>,
}

/// The `custom_genesis_export` tool allows exporting initial writes, storage logs, and factory dependencies
/// from the ZKSync PostgreSQL database in a way that they can be used as a custom genesis state for a new chain.
///
/// Inputs:
///     * `database_url` - URL to the PostgreSQL database.
///     * `output` - Path to the output file.
///     * `genesis_config_path` - Path to the `genesis.yaml` configuration file, which will be used to set up a new chain (located in the `file_based` directory).
///
/// Given the inputs above, `custom_genesis_export` will perform the following:
///     * Read initial writes, storage logs, and factory dependencies; filter out those related to the system context,
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

    let mut out = BufWriter::new(File::create(&args.output_path)?);

    println!(
        "Export file: {}",
        args.output_path.canonicalize()?.display(),
    );

    println!("Connecting to source database...");
    let mut conn_source =
        PgConnection::connect(&args.database_url.or_else(|| std::env::var("DATABASE_URL").ok()).expect("Specify the database connection string in either a CLI argument or in the DATABASE_URL environment variable."))
            .await?;
    println!("Connected to source database.");

    println!("Reading initial writes...");
    let count_initial_writes: i64 = sqlx::query("select count(*) from initial_writes;")
        .fetch_one(&mut conn_source)
        .await?
        .get(0);
    let mut initial_writes =
        sqlx::query_as::<_, InitialWriteRow>("select hashed_key, index from initial_writes;")
            .fetch(&mut conn_source);

    // write count of initial writes
    out.write_all(&i64::to_le_bytes(count_initial_writes))?;
    let mut actual_initial_writes_count = 0;
    while let Some(r) = initial_writes.try_next().await? {
        out.write_all(&r.hashed_key)?;
        out.write_all(&r.index.to_le_bytes())?;
        actual_initial_writes_count += 1;
    }
    if actual_initial_writes_count != count_initial_writes {
        panic!("Database reported {count_initial_writes} initial writes; only received {actual_initial_writes_count} for export.");
    }
    drop(initial_writes);

    println!("Exported {count_initial_writes} initial writes.");

    println!("Reading storage logs...");

    // skipping system context-related entries
    let count_storage_logs: i64 = sqlx::query(
        r#"
        select count(distinct hashed_key) from storage_logs
        where address <> '\x000000000000000000000000000000000000800b'::bytea or
            key in (
                '\x0000000000000000000000000000000000000000000000000000000000000000'::bytea,
                '\x0000000000000000000000000000000000000000000000000000000000000003'::bytea,
                '\x0000000000000000000000000000000000000000000000000000000000000004'::bytea,
                '\x0000000000000000000000000000000000000000000000000000000000000005'::bytea
            );"#,
    )
    .fetch_one(&mut conn_source)
    .await?
    .get(0);
    out.write_all(&i64::to_le_bytes(count_storage_logs))?;

    let mut storage_logs = sqlx::query_as::<_, StorageLogRow>(
        r#"
        select address, key, value
        from storage_logs sl
        where miniblock_number = (select max(miniblock_number) from storage_logs where hashed_key = sl.hashed_key)
          and (
            address <> '\x000000000000000000000000000000000000800b'::bytea or
            key in (
                    '\x0000000000000000000000000000000000000000000000000000000000000000'::bytea,
                    '\x0000000000000000000000000000000000000000000000000000000000000003'::bytea,
                    '\x0000000000000000000000000000000000000000000000000000000000000004'::bytea,
                    '\x0000000000000000000000000000000000000000000000000000000000000005'::bytea
                )
            );"#,
    )
    .fetch(&mut conn_source);

    let mut actual_storage_logs_count = 0;

    // we need to keep this collection in memory to calculate hashes for genesis in the end
    let mut storage_logs_for_genesis: Vec<StorageLog> =
        Vec::with_capacity(count_storage_logs as usize);

    while let Some(r) = storage_logs.try_next().await? {
        out.write_all(&r.address)?;
        out.write_all(&r.key)?;
        out.write_all(&r.value)?;
        actual_storage_logs_count += 1;
        storage_logs_for_genesis.push(r.into());
    }
    if actual_storage_logs_count != count_storage_logs {
        panic!("Retrieved {actual_storage_logs_count} storage logs from the database; expected {count_storage_logs}.");
    }

    println!("Exported {count_storage_logs} storage logs from source database.");

    drop(storage_logs);

    println!("Loading factory deps from source database...");
    let count_factory_deps: i64 = sqlx::query("select count(*) from factory_deps;")
        .fetch_one(&mut conn_source)
        .await?
        .get(0);
    out.write_all(&i64::to_le_bytes(count_factory_deps))?;

    let mut factory_deps =
        sqlx::query_as::<_, FactoryDepRow>("select bytecode_hash, bytecode from factory_deps;")
            .fetch(&mut conn_source);

    let mut actual_factory_deps_count = 0;
    while let Some(r) = factory_deps.try_next().await? {
        out.write_all(&r.bytecode_hash)?;
        out.write_all(&(r.bytecode.len() as u64).to_le_bytes())?;
        out.write_all(&r.bytecode)?;
        actual_factory_deps_count += 1;
    }
    if actual_factory_deps_count != count_factory_deps {
        panic!("Retrieved {actual_factory_deps_count} factory deps from the database; expected {count_factory_deps}.");
    }
    drop(factory_deps);

    println!("Exported {count_factory_deps} factory deps from source database.");

    conn_source.close().await?;

    println!("Calculating new genesis parameters");

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

    let (genesis_batch_params, _) = make_genesis_batch_params(
        storage_logs_for_genesis.as_slice(),
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

    let bytes =
        encode_yaml_repr::<zksync_protobuf_config::proto::genesis::Genesis>(&genesis_config)?;
    fs::write(&args.genesis_config_path, &bytes)?;

    println!("Done.");

    Ok(())
}

impl From<StorageLogRow> for StorageLog {
    fn from(value: StorageLogRow) -> Self {
        StorageLog::new_write_log(
            StorageKey::new(AccountTreeId::new(H160(value.address)), H256(value.key)),
            H256(value.value),
        )
    }
}
