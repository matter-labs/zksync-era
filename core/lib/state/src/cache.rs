// necessary for usize to f64 convertions for metrics
#![allow(clippy::cast_precision_loss)]

use std::{hash::Hash, time::Instant};

type MokaBase<K, V> = mini_moka::sync::Cache<K, V>;

/// Trait for values that can be put into [`Cache`]. The type param denotes the key type.
pub trait CacheValue<K>: Clone + Send + Sync {
    /// Weight of this value that determines when the cache LRU logic kicks in. Should be
    /// exactly or approximately equal to the total byte size of the value, including heap-allocated
    /// data (and, potentially, the key byte size if the value size is always small).
    fn cache_weight(&self) -> u32;
}

/// [`Cache`] implementation that uses LRU eviction policy.
#[derive(Debug, Clone)]
pub struct Cache<K: Eq + Hash, V> {
    name: &'static str,
    cache: Option<MokaBase<K, V>>,
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Send + Sync + 'static,
    V: CacheValue<K> + 'static,
{
    /// Creates a new cache.
    ///
    /// # Panics
    ///
    /// Panics if an invalid cache capacity is provided.
    pub fn new(name: &'static str, capacity: u64) -> Self {
        let cache = if capacity == 0 {
            None
        } else {
            Some(
                MokaBase::<K, V>::builder()
                    .weigher(|_, value| value.cache_weight())
                    .max_capacity(capacity)
                    .build(),
            )
        };

        Self { name, cache }
    }

    /// Gets an entry and pulls it to the front if it exists.
    pub fn get(&self, key: &K) -> Option<V> {
        let start_instant = Instant::now();
        let entry = self.cache.as_ref()?.get(key);
        // ^ We intentionally don't report metrics if there's no real cache.

        metrics::histogram!(
            "server.state_cache.latency",
            start_instant.elapsed(),
            "name" => self.name,
            "method" => "get",
        );
        metrics::increment_counter!(
            "server.state_cache.requests",
            "name" => self.name,
            "kind" => if entry.is_some() { "hit" } else { "miss" }
        );

        entry
    }

    /// Pushes an entry and performs LRU cache operations.
    pub fn insert(&self, key: K, value: V) {
        let start_instant = Instant::now();
        let Some(cache) = self.cache.as_ref() else {
            return;
        };
        // ^ We intentionally don't report metrics if there's no real cache.
        cache.insert(key, value);

        metrics::histogram!(
            "server.state_cache.latency",
            start_instant.elapsed(),
            "name" => self.name,
            "method" => "insert"
        );
        self.report_size();
    }

    pub(crate) fn report_size(&self) {
        if let Some(cache) = &self.cache {
            metrics::gauge!("server.state_cache.len", cache.entry_count() as f64, "name" => self.name);
            metrics::gauge!(
                "server.state_cache.used_memory",
                cache.weighted_size() as f64,
                "name" => self.name,
            );
        }
    }

    /// Removes the specified key from this cache.
    pub fn remove(&self, key: &K) {
        if let Some(cache) = &self.cache {
            cache.invalidate(key);
        }
    }

    /// Removes all entries from this cache.
    pub fn clear(&self) {
        if let Some(cache) = &self.cache {
            cache.invalidate_all();
            self.report_size();
        }
    }

    #[cfg(test)]
    pub(crate) fn estimated_len(&self) -> u64 {
        self.cache.as_ref().map_or(0, MokaBase::entry_count)
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::H256;

    use super::*;

    #[test]
    fn cache_with_zero_capacity() {
        let zero_cache = Cache::<H256, Vec<u8>>::new("test", 0);
        zero_cache.insert(H256::zero(), vec![1, 2, 3]);
        assert_eq!(zero_cache.get(&H256::zero()), None);

        // The zero-capacity `MokaBase` cache can actually contain items temporarily!
        let not_quite_zero_cache = MokaBase::<H256, Vec<u8>>::builder()
            .weigher(|_, value| value.cache_weight())
            .max_capacity(0)
            .build();
        not_quite_zero_cache.insert(H256::zero(), vec![1, 2, 3]);
        assert_eq!(not_quite_zero_cache.get(&H256::zero()), Some(vec![1, 2, 3]));
        // The item is evicted after the first access.
        assert_eq!(not_quite_zero_cache.get(&H256::zero()), None);
    }
}
