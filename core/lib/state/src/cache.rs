// necessary for usize to f64 convertions for metrics
#![allow(clippy::cast_precision_loss)]

use std::{hash::Hash, time::Instant};

type MokaBase<K> = mini_moka::sync::Cache<K, Vec<u8>>;

/// [`Cache`] implementation that uses LRU eviction policy.
#[derive(Debug, Clone)]
pub struct Cache<K: Hash + Eq + Send + Sync> {
    name: &'static str,
    cache: MokaBase<K>,
}

impl<K: Hash + Eq + Send + Sync + 'static> Cache<K> {
    /// Creates a new cache.
    ///
    /// # Panics
    ///
    /// Panics if an invalid cache capacity (usize) is provided.
    pub fn new(name: &'static str, capacity_mb: usize) -> Self {
        let cache = MokaBase::<K>::builder()
            .weigher(|_, value| -> u32 { value.len().try_into().unwrap_or(u32::MAX) })
            .max_capacity((capacity_mb * 1_000_000) as u64)
            .build();

        Self { name, cache }
    }

    /// Gets an entry and pulls it to the front if it exists.
    pub fn get(&self, key: &K) -> Option<Vec<u8>> {
        let start_instant = Instant::now();
        let entry = self.cache.get(key);
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
    pub fn insert(&self, key: K, value: Vec<u8>) {
        let start_instant = Instant::now();
        self.cache.insert(key, value);
        metrics::histogram!(
            "server.state_cache.latency",
            start_instant.elapsed(),
            "name" => self.name,
            "method" => "insert"
        );

        metrics::gauge!("server.state_cache.len", self.cache.entry_count() as f64, "name" => self.name);
        metrics::gauge!(
            "server.state_cache.used_memory",
            self.cache.weighted_size() as f64,
            "name" => self.name,
        );
    }
}
