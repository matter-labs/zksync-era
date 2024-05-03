//! Generic cache abstraction used by storage implementations.

pub mod lru_cache;
mod metrics;
pub mod sequential_cache;

type MokaBase<K, V> = mini_moka::sync::Cache<K, V>;

/// Trait for values that can be put into caches. The type param denotes the key type.
pub trait CacheValue<K>: Clone + Send + Sync {
    /// Weight of this value that determines when the cache LRU logic kicks in. Should be
    /// exactly or approximately equal to the total byte size of the value, including heap-allocated
    /// data (and, potentially, the key byte size if the value size is always small).
    fn cache_weight(&self) -> u32;
}
