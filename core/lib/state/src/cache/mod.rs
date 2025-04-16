//! Generic cache abstraction used by storage implementations.

pub mod lru_cache;
mod metrics;
pub mod sequential_cache;

type MokaBase<K, V> = mini_moka::sync::Cache<K, V>;
