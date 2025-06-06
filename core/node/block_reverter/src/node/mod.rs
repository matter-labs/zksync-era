//! Dependency injection for the block reverter.

pub use self::{
    block_reverter::BlockReverterLayer, resources::BlockReverterResource,
    unconditional_revert::UnconditionalRevertLayer,
};

mod block_reverter;
mod resources;
mod unconditional_revert;
