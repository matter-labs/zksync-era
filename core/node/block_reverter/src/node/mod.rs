//! Dependency injection for the block reverter.

pub use self::{block_reverter::BlockReverterLayer, resources::BlockReverterResource};

mod block_reverter;
mod resources;
