#![feature(array_chunks)]
#![feature(iter_next_chunk)]
mod processor;

mod storage;

mod l1_fetcher;

mod utils;

pub use crate::{
    l1_fetcher::{
        blob_http_client::{BlobClient, LocalDbBlobSource},
        main_node_client::{L1RecoveryDetachedMainNodeClient, L1RecoveryOnlineMainNodeClient},
        types::CommitBlock,
    },
    processor::db_recovery::{
        create_l1_snapshot, insert_dummy_l1_batch, recover_eth_sender, recover_eth_watch,
        recover_latest_protocol_version,
    },
};
