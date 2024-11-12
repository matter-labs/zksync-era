use std::collections::HashMap;

use sqlx::prelude::*;
use sqlx::PgConnection;
use sqlx::QueryBuilder;
use zksync_types::{AccountTreeId, StorageKey, StorageLog, StorageLogKind, H256};

const BIND_LIMIT: usize = 65535; // postgres limit = 65535

static SOURCE_DATABASE_URL: &str =
    "postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era";
static DESTINATION_DATABASE_URL: &str =
    "postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_imex_destination";

type FactoryDeps = HashMap<H256, Vec<u8>>;

#[tokio::main]
async fn main() {
    println!("Connecting to source database...");
    let mut conn_source = PgConnection::connect(SOURCE_DATABASE_URL).await.unwrap();
    println!("Connected to source database.");

    println!("Reading initial writes...");
    #[derive(FromRow)]
    struct InitialWriteRow {
        hashed_key: [u8; 32],
        l1_batch_number: i32,
        index: i32,
    }
    let initial_writes = sqlx::query_as::<_, InitialWriteRow>(
        "select hashed_key, l1_batch_number, index from initial_writes;",
    )
    .fetch_all(&mut conn_source)
    .await
    .unwrap();
    println!(
        "Loaded {} initial writes from source database.",
        initial_writes.len(),
    );

    println!("Reading storage logs...");
    let storage_logs = sqlx::query(r#"
        select distinct iw.hashed_key,
                        address,
                        key,
                        last_value(value) over (partition by iw.hashed_key order by miniblock_number) as value
        from initial_writes iw
                join storage_logs sl on sl.hashed_key = iw.hashed_key;
                "#)
        .fetch_all(&mut conn_source)
        .await
        .unwrap()
        .into_iter()
        .map(|r| StorageLog {
            kind: StorageLogKind::InitialWrite,
            key: StorageKey::new(
                AccountTreeId::from_fixed_bytes(r.get("address")),
                H256(r.get("key")),
            ),
            value: H256(r.get("value")),
        })
        .collect::<Vec<_>>();
    println!(
        "Loaded {} storage logs from source database.",
        storage_logs.len(),
    );

    println!("Loading factory deps from source database...");
    let factory_deps: FactoryDeps =
        sqlx::query("select bytecode_hash, bytecode from factory_deps fd;")
            .fetch_all(&mut conn_source)
            .await
            .unwrap()
            .into_iter()
            .map(|r| (H256(r.get("bytecode_hash")), r.get("bytecode")))
            .collect();
    println!(
        "Loaded {} factory deps from source database.",
        factory_deps.len(),
    );

    conn_source.close().await.unwrap();

    println!("Connecting to destination database...");
    let mut conn_destination = PgConnection::connect(DESTINATION_DATABASE_URL)
        .await
        .unwrap();
    println!("Connected to destination database.");

    // insert initial writes
    const IW_BINDINGS_PER_BATCH: usize = 3;
    let iw_batches = initial_writes.chunks(BIND_LIMIT / IW_BINDINGS_PER_BATCH);
    println!(
        "Copying initial writes to destination in {} batches...",
        iw_batches.len(),
    );

    let mut total_iw_insertions = 0;
    for (i, batch) in iw_batches.enumerate() {
        let mut q = QueryBuilder::new(
            r#"
                insert into initial_writes(hashed_key, l1_batch_number, index)
            "#,
        );

        q.push_values(batch, |mut args_list, value| {
            args_list
                .push_bind(value.hashed_key)
                .push_bind(value.l1_batch_number) // TODO: should the batch number reset to zero or should it be copied over?
                .push_bind(value.index);
        });

        q.build().execute(&mut conn_destination).await.unwrap();
        total_iw_insertions += batch.len();
        println!("Inserted batch {i} to destination database, for total of {total_iw_insertions} initial write records inserted.");
    }
    println!("Finished inserting initial writes to destination database.");

    // insert storage logs
    const SL_BINDINGS_PER_BATCH: usize = 7;
    let sl_batches = storage_logs.chunks(BIND_LIMIT / SL_BINDINGS_PER_BATCH);
    println!(
        "Copying storage logs to destination in {} batches...",
        sl_batches.len(),
    );

    let mut total_sl_insertions = 0;
    for (i, batch) in sl_batches.enumerate() {
        let mut q = QueryBuilder::new(
            r#"
                insert into storage_logs(hashed_key, address, key, value, operation_number, tx_hash, miniblock_number)
            "#,
        );

        q.push_values(batch, |mut args_list, value| {
            args_list
                .push_bind(value.key.hashed_key().0)
                .push_bind(value.key.address().0)
                .push_bind(value.key.key().0)
                .push_bind(value.value.0)
                .push_bind(todo!("operation number") as i32)
                .push_bind(todo!("transaction hash") as i32)
                .push_bind(todo!("miniblock number") as i32);
        });

        q.build().execute(&mut conn_destination).await.unwrap();
        total_sl_insertions += batch.len();
        println!("Inserted batch {i} to destination database, for total of {total_sl_insertions} storage log records inserted.");
    }
    println!("Finished inserting storage logs to destination database.");
}
