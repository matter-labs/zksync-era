use std::{
    cell::OnceCell,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::OnceLock,
};

use clap::Parser;
use futures::TryStreamExt;
use sqlx::{prelude::*, Connection, PgConnection};
use zksync_types::{
    get_system_context_init_logs, get_system_context_key, H256, SYSTEM_CONTEXT_ADDRESS,
    SYSTEM_CONTEXT_BLOCK_GAS_LIMIT_POSITION, SYSTEM_CONTEXT_CHAIN_ID_POSITION,
    SYSTEM_CONTEXT_COINBASE_POSITION, SYSTEM_CONTEXT_DIFFICULTY_POSITION,
};

#[derive(Debug, Parser)]
#[command(name = "Custom genesis export tool", author = "Matter Labs")]
struct Args {
    /// PostgreSQL connection string for the database to export.
    #[arg(short, long)]
    database_url: Option<String>,

    /// Output file path.
    #[arg(short, long, default_value = "gexport.bin")]
    output: PathBuf,
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

static SYSTEM_CONTEXT_INIT_LOGS: OnceLock<Vec<[u8; 32]>> = OnceLock::new();

fn should_export_storage_row(row: &StorageLogRow) -> bool {
    if row.address != SYSTEM_CONTEXT_ADDRESS.0 {
        return true;
    }

    let allow_system_context_keys = SYSTEM_CONTEXT_INIT_LOGS.get_or_init(|| {
        get_system_context_init_logs(
            Default::default(), // doesn't matter because we're only reading the keys
        )
        .iter()
        .map(|l| l.key.key().0)
        .collect()
    });

    allow_system_context_keys.contains(&row.key)
}

#[tokio::main]
async fn main() {
    // TODO: paginate, bc probably cannot store 25gb in memory
    let args = Args::parse();

    let mut out = BufWriter::new(File::create_new(&args.output).unwrap());

    println!(
        "Export file: {}",
        args.output.canonicalize().unwrap().display(),
    );

    println!("Connecting to source database...");
    let mut conn_source =
        PgConnection::connect(&args.database_url.or_else(|| std::env::var("DATABASE_URL").ok()).expect("Specify the database connection string in either a CLI argument or in the DATABASE_URL environment variable."))
            .await
            .unwrap();
    println!("Connected to source database.");

    println!("Reading initial writes...");
    let count_initial_writes: i64 = sqlx::query("select count(*) from initial_writes;")
        .fetch_one(&mut conn_source)
        .await
        .unwrap()
        .get(0);
    let mut initial_writes =
        sqlx::query_as::<_, InitialWriteRow>("select hashed_key, index from initial_writes;")
            .fetch(&mut conn_source);

    // write count of initial writes
    out.write_all(&i64::to_le_bytes(count_initial_writes))
        .unwrap();
    let mut actual_initial_writes_count = 0;
    while let Some(r) = initial_writes.try_next().await.unwrap() {
        out.write_all(&r.hashed_key).unwrap();
        out.write_all(&r.index.to_le_bytes()).unwrap();
        actual_initial_writes_count += 1;
    }
    if actual_initial_writes_count != count_initial_writes {
        panic!("Database reported {count_initial_writes} initial writes; only received {actual_initial_writes_count} for export.");
    }
    drop(initial_writes);

    println!("Exported {count_initial_writes} initial writes.");

    println!("Reading storage logs...");
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
    .await
    .unwrap()
    .get(0);
    out.write_all(&i64::to_le_bytes(count_storage_logs))
        .unwrap();

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
    while let Some(r) = storage_logs.try_next().await.unwrap() {
        out.write_all(&r.address).unwrap();
        out.write_all(&r.key).unwrap();
        out.write_all(&r.value).unwrap();
        actual_storage_logs_count += 1;
    }
    if actual_storage_logs_count != count_storage_logs {
        panic!("Retrieved {actual_storage_logs_count} storage logs from the database; expected {count_storage_logs}.");
    }
    drop(storage_logs);

    println!("Exported {count_storage_logs} storage logs from source database.");

    println!("Loading factory deps from source database...");
    let count_factory_deps: i64 = sqlx::query("select count(*) from factory_deps;")
        .fetch_one(&mut conn_source)
        .await
        .unwrap()
        .get(0);
    out.write_all(&i64::to_le_bytes(count_factory_deps))
        .unwrap();

    let mut factory_deps =
        sqlx::query_as::<_, FactoryDepRow>("select bytecode_hash, bytecode from factory_deps;")
            .fetch(&mut conn_source);

    let mut actual_factory_deps_count = 0;
    while let Some(r) = factory_deps.try_next().await.unwrap() {
        out.write_all(&r.bytecode_hash).unwrap();
        out.write_all(&(r.bytecode.len() as u64).to_le_bytes())
            .unwrap();
        out.write_all(&r.bytecode).unwrap();
        actual_factory_deps_count += 1;
    }
    if actual_factory_deps_count != count_factory_deps {
        panic!("Retrieved {actual_factory_deps_count} factory deps from the database; expected {count_factory_deps}.");
    }
    drop(factory_deps);

    println!("Exported {count_factory_deps} factory deps from source database.");

    conn_source.close().await.unwrap();

    println!("Done.");
}
