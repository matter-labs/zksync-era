use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

use clap::Parser;
use futures::TryStreamExt;
use sqlx::{prelude::*, Connection, PgConnection};

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
    #[derive(FromRow)]
    struct InitialWriteRow {
        hashed_key: [u8; 32],
        index: i64,
    }
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
    #[derive(FromRow)]
    struct StorageLogRow {
        address: [u8; 20],
        key: [u8; 32],
        value: [u8; 32],
    }
    let count_storage_logs: i64 =
        sqlx::query("select count(distinct hashed_key) from storage_logs;")
            .fetch_one(&mut conn_source)
            .await
            .unwrap()
            .get(0);
    let mut storage_logs = sqlx::query_as::<_, StorageLogRow>(
        r#"
        select address, key, value
        from storage_logs sl
        where miniblock_number = (
            select max(miniblock_number)
            from storage_logs
            where hashed_key = sl.hashed_key
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
    #[derive(FromRow)]
    struct FactoryDepRow {
        bytecode_hash: [u8; 32],
        bytecode: Vec<u8>,
    }
    let count_factory_deps: i64 = sqlx::query("select count(*) from factory_deps;")
        .fetch_one(&mut conn_source)
        .await
        .unwrap()
        .get(0);
    let mut factory_deps =
        sqlx::query_as::<_, FactoryDepRow>("select bytecode_hash, bytecode from factory_deps;")
            .fetch(&mut conn_source);

    let mut actual_factory_deps_count = 0;
    while let Some(r) = factory_deps.try_next().await.unwrap() {
        out.write_all(&r.bytecode_hash).unwrap();
        out.write_all(&bincode::serialize(&r.bytecode).unwrap())
            .unwrap();
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
