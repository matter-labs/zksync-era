use std::collections::hash_map::{Entry, HashMap};

use clap::Parser;

use zksync_config::PostgresConfig;
use zksync_dal::ConnectionPool;
use zksync_env_config::FromEnv;
use zksync_types::{MiniblockNumber, H256};

/// When the threshold is reached then the migration is blocked on vacuuming.
const UNVACUUMED_ROWS_THRESHOLD: usize = 2_000_000;

#[derive(Debug, Parser)]
#[command(
    author = "Matter Labs",
    about = "Migration that deduplicates rows in storage_logs DB table"
)]
struct Cli {
    /// Miniblock number to start migration from.
    #[arg(long)]
    start_from_miniblock: u32,
}

/// Blockchain state cache
struct StateCache {
    /// (hashed_key => value) mapping.
    pub storage: HashMap<H256, H256>,
    /// Miniblock number cache is valid for.
    pub miniblock: Option<MiniblockNumber>,
    /// Flag indicating if state is initially empty.
    pub is_state_initially_empty: bool,
}

impl StateCache {
    /// Loads value from state if present.
    pub fn get_value(&mut self, hashed_key: H256) -> Option<H256> {
        if let Entry::Vacant(e) = self.storage.entry(hashed_key) {
            if self.is_state_initially_empty {
                e.insert(H256::zero());
            }
        }

        self.storage.get(&hashed_key).copied()
    }
}

#[tokio::main]
async fn main() {
    let config = PostgresConfig::from_env().unwrap();
    let opt = Cli::parse();
    let pool = ConnectionPool::singleton(config.master_url().unwrap())
        .build()
        .await
        .unwrap();
    let mut connection = pool.access_storage().await.unwrap();

    let sealed_miniblock = connection
        .blocks_dal()
        .get_sealed_miniblock_number()
        .await
        .unwrap();
    println!(
        "Migration started for miniblock range {}..={}",
        opt.start_from_miniblock, sealed_miniblock
    );

    let (previous_miniblock, is_state_initially_empty) = if opt.start_from_miniblock == 0 {
        (None, true)
    } else {
        (Some((opt.start_from_miniblock - 1).into()), false)
    };

    let mut state_cache = StateCache {
        storage: HashMap::new(),
        miniblock: previous_miniblock,
        is_state_initially_empty,
    };

    let mut number_of_unvacuum_rows = 0;

    for miniblock_number in opt.start_from_miniblock..=sealed_miniblock.0 {
        let miniblock_number = MiniblockNumber(miniblock_number);

        // Load all storage logs of miniblock.
        let storage_logs = connection
            .storage_logs_dal()
            .get_miniblock_storage_logs(miniblock_number)
            .await;
        let initial_storage_logs_count = storage_logs.len();

        // Load previous values from memory.
        let prev_values: HashMap<_, _> = storage_logs
            .iter()
            .map(|(hashed_key, _, _)| (*hashed_key, state_cache.get_value(*hashed_key)))
            .collect();

        // Load missing previous values from database.
        let missing_keys: Vec<_> = prev_values
            .iter()
            .filter_map(|(key, value)| (value.is_none()).then_some(*key))
            .collect();

        let in_memory_prev_values_iter = prev_values.into_iter().filter_map(|(k, v)| Some((k, v?)));
        let prev_values: HashMap<_, _> = if miniblock_number.0 == 0 || missing_keys.is_empty() {
            assert!(missing_keys.is_empty());
            in_memory_prev_values_iter.collect()
        } else {
            let values_for_missing_keys: HashMap<_, _> = connection
                .storage_logs_dal()
                .get_storage_values(&missing_keys, miniblock_number - 1)
                .await;

            in_memory_prev_values_iter
                .chain(
                    values_for_missing_keys
                        .into_iter()
                        .map(|(k, v)| (k, v.unwrap_or_else(H256::zero))),
                )
                .collect()
        };

        // Effective state for keys that were touched in the current miniblock.
        let current_values: HashMap<_, _> = storage_logs
            .into_iter()
            .map(|(hashed_key, value, operation_number)| (hashed_key, (value, operation_number)))
            .collect();

        // Collect effective storage logs of the miniblock and their operation numbers.
        let (effective_logs, op_numbers_to_retain): (Vec<_>, Vec<_>) = current_values
            .into_iter()
            .filter_map(|(hashed_key, (value, operation_number))| {
                let prev_value = prev_values[&hashed_key];
                (value != prev_value).then_some(((hashed_key, value), operation_number as i32))
            })
            .unzip();

        // Remove others, i.e. non-effective logs from DB.
        connection
            .storage_logs_dal()
            .retain_storage_logs(miniblock_number, &op_numbers_to_retain)
            .await;
        number_of_unvacuum_rows += initial_storage_logs_count - op_numbers_to_retain.len();

        // Update state cache.
        for (key, value) in effective_logs {
            state_cache.storage.insert(key, value);
        }
        state_cache.miniblock = Some(miniblock_number);

        if miniblock_number.0 < 100 || miniblock_number.0 % 100 == 0 {
            println!("Deduplicated logs for miniblock {miniblock_number}, number of unvacuumed rows {number_of_unvacuum_rows}");
        }

        if number_of_unvacuum_rows > UNVACUUMED_ROWS_THRESHOLD {
            let started_at = std::time::Instant::now();
            println!("Starting vacuuming");
            connection.storage_logs_dal().vacuum_storage_logs().await;
            number_of_unvacuum_rows = 0;
            println!("Vacuum finished in {:?}", started_at.elapsed());
        }
    }

    if number_of_unvacuum_rows > 0 {
        let started_at = std::time::Instant::now();
        println!("Starting vacuuming");
        connection.storage_logs_dal().vacuum_storage_logs().await;
        println!("Vacuum finished in {:?}", started_at.elapsed());
    }

    println!("Finished");
}
