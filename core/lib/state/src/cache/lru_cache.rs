use std::hash::Hash;

use crate::cache::{
    metrics::{LruCacheConfig, Method, RequestOutcome, METRICS},
    CacheValue, MokaBase,
};

/// Cache implementation that uses LRU eviction policy.
#[derive(Debug, Clone)]
pub struct LruCache<K: Eq + Hash, V> {
    name: &'static str,
    cache: Option<MokaBase<K, V>>,
}

impl<K, V> LruCache<K, V>
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
        tracing::info!("Configured LRU cache `{name}` with capacity {capacity}B");
        if let Err(err) = METRICS.lru_info[&name].set(LruCacheConfig { capacity }) {
            tracing::warn!(
                "LRU cache `{name}` was already created with config {:?}; new config: {:?}",
                METRICS.lru_info[&name].get(),
                err.into_inner()
            );
        }

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

    /// Returns the capacity of this cache in bytes.
    pub fn capacity(&self) -> u64 {
        self.cache
            .as_ref()
            .map_or(0, |cache| cache.policy().max_capacity().unwrap_or(u64::MAX))
    }

    /// Gets an entry and pulls it to the front if it exists.
    pub fn get(&self, key: &K) -> Option<V> {
        let latency = METRICS.latency[&(self.name, Method::Get)].start();
        let entry = self.cache.as_ref()?.get(key);
        // ^ We intentionally don't report metrics if there's no real cache.

        latency.observe();
        let request_outcome = if entry.is_some() {
            RequestOutcome::Hit
        } else {
            RequestOutcome::Miss
        };
        METRICS.requests[&(self.name, request_outcome)].inc();

        entry
    }

    /// Pushes an entry and performs LRU cache operations.
    pub fn insert(&self, key: K, value: V) {
        let latency = METRICS.latency[&(self.name, Method::Insert)].start();
        let Some(cache) = self.cache.as_ref() else {
            return;
        };
        // ^ We intentionally don't report metrics if there's no real cache.
        cache.insert(key, value);

        latency.observe();
        self.report_size();
    }

    pub(crate) fn report_size(&self) {
        if let Some(cache) = &self.cache {
            METRICS.len[&self.name].set(cache.entry_count());
            METRICS.used_memory[&self.name].set(cache.weighted_size());
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

    use crate::cache::{lru_cache::LruCache, *};

    impl CacheValue<H256> for Vec<u8> {
        fn cache_weight(&self) -> u32 {
            self.len().try_into().expect("Cached bytes are too large")
        }
    }

    #[test]
    fn cache_with_zero_capacity() {
        let zero_cache = LruCache::<H256, Vec<u8>>::new("test", 0);
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
