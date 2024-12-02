#![feature(allocator_api)]

use std::time::{SystemTime, UNIX_EPOCH};
pub use self::keeper::ZkosStateKeeper;

mod keeper;
mod tree;
mod preimage_source;
mod seal_logic;


pub fn millis_since_epoch() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Incorrect system time")
        .as_millis()
}
