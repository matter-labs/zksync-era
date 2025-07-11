#![feature(allocator_api)]
#![feature(generic_const_exprs)]

use std::time::{SystemTime, UNIX_EPOCH};

pub use self::{
    io::{mempool::MempoolIO, OutputHandler, StateKeeperIO, StateKeeperPersistence},
    keeper::ZkosStateKeeper,
    seal_criteria::{ConditionalSealer, SequencerSealer},
    updates::{FinishedBlock, UpdatesManager},
};

mod batch_executor;
pub mod io;
mod keeper;
mod metrics;
mod seal_criteria;
pub mod state_keeper_storage;
mod updates;

pub fn millis_since_epoch() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Incorrect system time")
        .as_millis()
}
