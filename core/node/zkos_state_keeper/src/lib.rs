#![feature(allocator_api)]

use std::time::{SystemTime, UNIX_EPOCH};

pub use self::{
    io::{mempool::MempoolIO, OutputHandler, StateKeeperIO, StateKeeperPersistence},
    keeper::ZkosStateKeeper,
    updates::UpdatesManager,
};

pub mod io;
mod keeper;
mod updates;

pub fn millis_since_epoch() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Incorrect system time")
        .as_millis()
}
