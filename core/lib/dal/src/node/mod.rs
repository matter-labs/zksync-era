//! Dependency injection for DAL.

pub use self::{
    metrics::PostgresMetricsLayer,
    pools_layer::{PoolsLayer, PoolsLayerBuilder},
    resources::{MasterPool, PoolResource, ReplicaPool},
};

mod metrics;
mod pools_layer;
mod resources;
