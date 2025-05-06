//! Dependency injection for DAL.

pub use self::{
    pools::{MasterPool, PoolResource, ReplicaPool},
    pools_layer::{PoolsLayer, PoolsLayerBuilder},
};

mod pools;
mod pools_layer;
