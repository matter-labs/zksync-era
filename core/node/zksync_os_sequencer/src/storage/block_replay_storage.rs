// block_replay_wal.rs

use std::{path::Path, convert::TryInto};
use futures::Stream;
use futures::stream::{self, BoxStream, StreamExt};
use serde::{Serialize, Deserialize};
use zk_os_forward_system::run::{BatchContext, BatchOutput};
use zksync_storage::db::{NamedColumnFamily, WriteBatch};
use zksync_storage::RocksDB;
use zksync_types::Transaction;
use crate::model::{BlockCommand, ReplayRecord, TransactionSource};
use crate::execution::metrics::{BLOCK_REPLAY_ROCKS_DB_METRICS};

/// Column families for WAL storage of block replay commands.
#[derive(Copy, Clone, Debug)]
pub enum BlockReplayColumnFamily {
    Context,
    Txs,

    /// Serialized replay commands, keyed by block number (big-endian bytes).
    Replay,
    /// Stores the latest appended block number under a fixed key.
    Latest,
}

impl NamedColumnFamily for BlockReplayColumnFamily {
    const DB_NAME: &'static str = "block_replay_wal";
    const ALL: &'static [Self] = &[
        BlockReplayColumnFamily::Replay,
        BlockReplayColumnFamily::Context,
        BlockReplayColumnFamily::Txs,
        BlockReplayColumnFamily::Latest,
    ];

    fn name(&self) -> &'static str {
        match self {
            BlockReplayColumnFamily::Replay => "replay",
            BlockReplayColumnFamily::Context => "context",
            BlockReplayColumnFamily::Txs => "txs",
            BlockReplayColumnFamily::Latest => "latest",
        }
    }
}

/// A write-ahead log storing replay blocks for sequencer recovery.
#[derive(Clone, Debug)]
pub struct BlockReplayStorage {
    db: RocksDB<BlockReplayColumnFamily>,
}

impl BlockReplayStorage {
    /// Key under `Latest` CF for tracking the highest block number.
    const LATEST_KEY: &'static [u8] = b"latest_block";

    /// Opens (or creates) the WAL at the specified filesystem path.
    pub fn new(db: RocksDB<BlockReplayColumnFamily>) -> Self {
        let mut batch: WriteBatch<'_, BlockReplayColumnFamily> = db.new_write_batch();
        let from = 0u32.to_be_bytes();
        let to = 1000000u32.to_be_bytes();
        batch.delete_range_cf(
            BlockReplayColumnFamily::Replay,
            &from..&to,
        );
        db.write(batch).expect("Failed to write to WAL");


        Self { db }
    }

    /// Appends a replay command (context + raw transactions) to the WAL.
    /// Also updates the Latest CF. Returns the corresponding BlockCommand.

    // todo: blockcontext is just passthrough
    pub async fn append_replay(
        &self,
        block_output: BatchOutput,
        record: ReplayRecord,
    ) -> (BatchOutput, ReplayRecord) {
        let latency = BLOCK_REPLAY_ROCKS_DB_METRICS.get_latency.start();
        assert!(!record.transactions.is_empty());
        let current_latest_block = self.latest_block().unwrap_or(0);

        if record.context.block_number <= current_latest_block {
            tracing::info!("Not appending block {}: already exists in WAL", record.context.block_number);
            return (block_output, record);
        }

        let block_num = record.context.block_number;
        let key = block_num.to_be_bytes();

        // Prepare record
        let context_value = serde_json::to_vec(&record.context).expect("Failed to serialize ReplayRecord");
        let txs_value = serde_json::to_vec(&record.transactions).expect("Failed to serialize ReplayRecord");

        // Batch both writes: replay entry and latest pointer
        let mut batch: WriteBatch<'_, BlockReplayColumnFamily> = self.db.new_write_batch();
        batch.put_cf(BlockReplayColumnFamily::Latest, Self::LATEST_KEY, &key);
        batch.put_cf(BlockReplayColumnFamily::Context, &key, &context_value);
        batch.put_cf(BlockReplayColumnFamily::Txs, &key, &txs_value);

        self.db.write(batch).expect("Failed to write to WAL");
        latency.observe();
        (block_output, record)
    }

    /// Returns the greatest block number that has been appended, or None if empty.
    pub fn latest_block(&self) -> Option<u64> {
        if let Ok(Some(bytes)) = self.db.get_cf(BlockReplayColumnFamily::Latest, Self::LATEST_KEY) {
            if bytes.len() == 8 {
                let arr: [u8; 8] = bytes.as_slice().try_into().ok()?;
                return Some(u64::from_be_bytes(arr));
            }
        }
        None
    }


    pub fn get_context(&self, block_number: u64) -> Option<BatchContext> {
        let key = block_number.to_be_bytes();
        if let Ok(Some(bytes)) = self.db.get_cf(BlockReplayColumnFamily::Context, &key) {
            serde_json::from_slice(&bytes).ok()
        } else {
            None
        }
    }

    pub fn get_replay_record(&self, block_number: u64) -> Option<ReplayRecord> {
        let key = block_number.to_be_bytes();
        if let Ok(Some(bytes_context)) = self.db.get_cf(BlockReplayColumnFamily::Context, &key) {
            if let Ok(Some(bytes_txs)) = self.db.get_cf(BlockReplayColumnFamily::Txs, &key) {
                Some(ReplayRecord {
                    context: serde_json::from_slice(&bytes_context).ok()?,
                    transactions: serde_json::from_slice(&bytes_txs).ok()?,
                })
            } else {
                None
            }
        } else {
            None
        }
    }
    /// Streams all replay commands with block_number ≥ `start`, in ascending block order.
    ///
    /// todo: some dirty code
    pub fn replay_commands_from<'a>(&'a self, start: u64) -> impl Stream<Item=BlockCommand> + 'a {
        let start_key = start.to_be_bytes();
        let iter = self.db.from_iterator_cf(
            BlockReplayColumnFamily::Context,
            std::ops::RangeFrom { start: &start_key[..] },
        );
        stream::iter(iter)
            .filter_map(move |(key_bytes, context_bytes)| {
                // Parse 8-byte block number
                if let Some(arr) = key_bytes.as_ref().get(0..8).and_then(|b| b.try_into().ok()) {
                    let block_num = u64::from_be_bytes(arr);
                    if block_num >= start {
                        // Deserialize record
                        if let Ok(context) = serde_json::from_slice::<BatchContext>(&context_bytes) {
                            let txs = self.db.get_cf(BlockReplayColumnFamily::Txs, &key_bytes).expect("").expect("");
                            let txs_parsed = serde_json::from_slice::<Vec<Transaction>>(&txs).expect("Failed to deserialize transactions");
                            let record = ReplayRecord{
                                context,
                                transactions: txs_parsed,
                            };
                            let cmd = BlockCommand::Replay(record);
                            return futures::future::ready(Some(cmd));
                        }
                    }
                }
                futures::future::ready(None)
            })
    }
}
