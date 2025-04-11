//! Execution of transaction in ZKsync Era

// Linter settings.
#![warn(missing_debug_implementations, missing_docs, bare_trait_objects)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::doc_markdown // frequent false positive: RocksDB
)]

pub use zksync_vm_interface::storage as interface;

pub use self::{
    cache::{lru_cache::LruCache, sequential_cache::SequentialCache},
    catchup::{AsyncCatchupTask, RocksdbCell},
    postgres::{PostgresStorage, PostgresStorageCaches, PostgresStorageCachesTask},
    rocksdb::{
        RocksdbStorage, RocksdbStorageBuilder, RocksdbStorageOptions, StateKeeperColumnFamily,
    },
    shadow_storage::ShadowStorage,
    storage_factory::{
        BatchDiff, BatchDiffs, CommonStorage, OwnedStorage, ReadStorageFactory, RocksdbWithMemory,
        SnapshotStorage,
    },
};

mod cache;
mod catchup;
mod postgres;
mod rocksdb;
mod shadow_storage;
mod storage_factory;
#[cfg(test)]
mod test_utils;
