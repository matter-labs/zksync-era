use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    time::Duration,
};

use ethers::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

pub(crate) const DEFAULT_BLOCK_RANGE: u64 = 50_000;

/// A single log entry we want to store.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct QueriedLog {
    /// The block number where this log was found (optional because logs can have None for pending blocks)
    pub(crate) block_number: Option<u64>,
    /// The transaction hash for the log
    pub(crate) transaction_hash: Option<H256>,
    /// The address this log was emitted from
    pub(crate) address: Address,
    /// All topics for the log
    pub(crate) topics: Vec<H256>,
    /// The data field of the log
    pub(crate) data: Vec<u8>,
}

/// A cache structure that keeps track of how far we’ve fetched and
/// also retains all queried logs in a vector.
#[derive(Debug, Serialize, Deserialize)]
struct Cache {
    first_seen_block: u64,
    last_seen_block: u64,
    all_logs: Vec<QueriedLog>,
}

/// Read a cache file from disk.
fn read_cache_from_file(cache_path: &str) -> Option<Cache> {
    let mut file = File::open(cache_path).ok()?;
    let mut data = vec![];
    file.read_to_end(&mut data).ok()?;
    serde_json::from_slice(&data).ok()
}

/// Write a cache file to disk.
fn write_cache_to_file(cache_path: &str, cache: &Cache) -> Result<(), Box<dyn std::error::Error>> {
    let serialized = serde_json::to_vec_pretty(cache)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(cache_path)?;
    file.write_all(&serialized)?;
    Ok(())
}

/// This function will:
/// 1. Read from (or initialize) a cache
/// 2. For each `(address, event_signature)` in `events_to_query`, build a filter
/// 3. Fetch logs in chunks of `block_range`
/// 4. Store each log's topics and data in the cache
/// 5. Write updates back to the cache
///
/// Returns the final vector of logs once all blocks have been processed.
pub(crate) async fn get_logs_for_events(
    block_to_start_with: u64,
    existing_cache_path: &str,
    rpc_url: &str,
    block_range: u64,
    events_to_query: &[(Address, &str, Option<H256>)], // (contract address, event signature, topic1)
) -> Vec<QueriedLog> {
    // ---------------------------------------------------------
    // 1. Read or initialize the cache
    // ---------------------------------------------------------
    let mut cache = read_cache_from_file(existing_cache_path).unwrap_or_else(|| Cache {
        first_seen_block: block_to_start_with,
        last_seen_block: block_to_start_with,
        all_logs: vec![],
    });

    // If the cache file was found, check the condition about `first_seen_block`
    if cache.first_seen_block > block_to_start_with {
        // If the cache's first_seen_block is larger than our new start,
        // clear the entire cache and reset.
        cache.first_seen_block = block_to_start_with;
        cache.last_seen_block = block_to_start_with;
        cache.all_logs.clear();
    }

    // ---------------------------------------------------------
    // 2. Connect to a provider
    // ---------------------------------------------------------
    let provider =
        Provider::<Http>::try_from(rpc_url).expect("Could not instantiate HTTP Provider");

    // Get the latest block so we know how far we can go
    let latest_block = provider
        .get_block_number()
        .await
        .expect("Failed to fetch latest block")
        .as_u64();

    // Our actual starting point is whichever is further along
    let mut current_block = cache.last_seen_block;

    // ---------------------------------------------------------
    // 3. Process logs in chunks of block_range
    // ---------------------------------------------------------
    while current_block <= latest_block {
        let start_of_range = current_block;
        let end_of_range = std::cmp::min(start_of_range + block_range, latest_block);

        println!("Processing range {start_of_range} - {end_of_range}\n");

        // If the entire range is below what we have already processed, skip
        if end_of_range < cache.last_seen_block {
            // skip range
            current_block = end_of_range + 1;
            println!("Range is cached, skipping...");
            continue;
        }

        // We'll collect all logs from all event filters in this chunk
        let mut new_logs_for_range = Vec::new();

        // ---------------------------------------------------------
        // 4. Build filters for each event signature and fetch logs
        // ---------------------------------------------------------
        for (contract_address, event_sig, topic_1) in events_to_query.iter() {
            // Example usage with ethers-rs: Filter::new().event(event_sig)
            // If your event signature is an "event Foo(address,uint256)" string,
            // ethers-rs will do the topic0 hashing automatically.
            // Alternatively, you can manually set .topics(Some(vec![event_sig_hash]), None, None, None).
            let mut filter = Filter::new()
                .address(*contract_address)
                .event(event_sig)
                .from_block(start_of_range)
                .to_block(end_of_range);

            if let Some(x) = topic_1 {
                filter = filter.topic1(*x);
            }

            // Sleep for 1 second before each JSON-RPC request to avoid hitting rate limits
            sleep(Duration::from_secs(1)).await;

            let logs = match provider.get_logs(&filter).await {
                Ok(ls) => ls,
                Err(e) => {
                    eprintln!(
                        "Failed to fetch logs for event signature {event_sig} at {contract_address:?}: {e}"
                    );
                    continue;
                }
            };

            // Store each log's topics + data
            for log in logs {
                new_logs_for_range.push(QueriedLog {
                    block_number: log.block_number.map(|bn| bn.as_u64()),
                    transaction_hash: log.transaction_hash,
                    address: log.address,
                    topics: log.topics,
                    data: log.data.to_vec(),
                });
            }
        }

        // ---------------------------------------------------------
        // 5. Update the cache, flush it to disk
        // ---------------------------------------------------------
        cache.last_seen_block = end_of_range;
        cache.all_logs.extend(new_logs_for_range);

        write_cache_to_file(existing_cache_path, &cache).expect("Failed to write cache to file");

        println!("Processed and saved the range!");

        // Move our current_block pointer forward
        if end_of_range == latest_block {
            break;
        } else {
            current_block = end_of_range + 1;
        }
    }

    // Return the logs we have in the cache. If you only want the new logs
    // from this run, you could track them differently. But for simplicity,
    // we return everything in the cache.
    cache.all_logs
}
