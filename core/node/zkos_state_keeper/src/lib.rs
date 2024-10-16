#![feature(allocator_api)]
pub use self::keeper::ZkOsStateKeeper;

mod keeper;
mod tree;
mod single_tx_source;
mod preimage_source;
mod zkos_conversions;