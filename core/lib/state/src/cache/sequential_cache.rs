use std::collections::VecDeque;

use crate::cache::metrics::{Method, RequestOutcome, METRICS};

/// A generic cache structure for storing key-value pairs in sequential order.
/// It allows for non-unique keys and supports efficient retrieval of values based on a key
/// threshold. The cache maintains a specified maximum capacity, removing the oldest entries
/// as new ones are added.
///
/// Usage example: storing mempool transactions and querying them by the `received_at` field
/// (clients maintain a cursor based on this key and poll the data structure for newer transactions)
///
/// K: Type of the ordered, potentially non-unique key. Example: Transaction's `received_at` field.
/// V: Type of the value associated with each key. Example: Transaction's hash.
#[derive(Debug, Clone)]
pub struct SequentialCache<K, V> {
    name: &'static str,
    data: VecDeque<(K, V)>,
    capacity: usize,
}

impl<K: Ord + Copy, V: Clone> SequentialCache<K, V> {
    /// Creates a new `SequentialCache` with the specified maximum capacity.
    pub fn new(name: &'static str, capacity: usize) -> Self {
        assert!(capacity > 0, "Cache capacity must be greater than 0");
        SequentialCache {
            name,
            data: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Inserts multiple key-value pairs into the cache from an iterator. If adding these
    /// items exceeds the cache's capacity, the oldest entries are removed. Keys can be non-unique.
    /// Returns `Err` when keys order is incorrect (a smaller key is inserted after a larger one)
    pub(crate) fn insert<I>(&mut self, items: I) -> anyhow::Result<()>
    where
        I: IntoIterator<Item = (K, V)>,
    {
        for (key, value) in items {
            let latency = METRICS.latency[&(self.name, Method::Insert)].start();
            anyhow::ensure!(
                Some(key) >= self.get_last_key(),
                "Keys must be inserted in sequential order"
            );
            if self.data.len() == self.capacity {
                self.data.pop_front();
            }
            self.data.push_back((key, value));
            latency.observe();
        }
        self.report_size();
        Ok(())
    }

    /// Queries and returns all values associated with keys strictly greater than the specified key.
    /// Returns `None` if cache cannot be used for this key -
    ///     that is, the oldest cache element is larger than the key requested, and we cannot guarantee
    ///     that there were no (dropped) elements between the requested key and the oldest hash element
    /// Otherwise returns `Some(results)`
    ///     with the cache tail starting from the requested key - but not including it.
    ///     Can be empty if the requested key is the largest in the cache.
    pub(crate) fn query(&self, after: K) -> Option<Vec<(K, V)>> {
        let latency = METRICS.latency[&(self.name, Method::Get)].start();
        let result = match self.data.partition_point(|&(key, _)| key <= after) {
            // All the cache elements are greater than the key provided - cannot use the cache
            0 => None,

            // Partition point found - will return all elements after it.
            // `partition_point` returns the first element for which predicate returns false.
            pos => Some(self.data.range(pos..).cloned().collect()),
        };
        latency.observe();
        METRICS.requests[&(self.name, RequestOutcome::from_hit(result.is_some()))].inc();
        result
    }

    /// Returns the last key in the cache
    pub(crate) fn get_last_key(&self) -> Option<K> {
        self.data.back().map(|&(key, _)| key)
    }

    /// Reports the number of entries to Prometheus.
    fn report_size(&self) {
        METRICS.len[&self.name].set(self.data.len() as u64);
    }
}

#[cfg(test)]
mod tests {
    use crate::cache::sequential_cache::SequentialCache;
    #[test]
    #[should_panic(expected = "Keys must be inserted in sequential order")]
    fn non_sequential_insertion() {
        let mut cache = SequentialCache::<u32, u32>::new("non_sequential_insertion", 3);
        cache
            .insert(vec![
                (1, 1),
                (3, 3), // note: 3 is inserted before 2, test should panic
                (2, 2),
            ])
            .unwrap();
    }

    #[test]
    fn non_sequential_insertion_multiple_invocations() {
        let mut cache = SequentialCache::<u32, u32>::new("non_sequential_insertion", 3);
        cache.insert(vec![(1, 1), (2, 2)]).unwrap();
        assert!(cache.insert(vec![(1, 1)]).is_err());
    }

    #[test]
    fn query() {
        let mut cache = SequentialCache::<u32, u32>::new("query", 100);
        cache.insert(vec![(1, 1), (2, 2), (2, 5), (3, 6)]).unwrap();
        cache.insert(vec![(3, 7), (100, 8)]).unwrap();
        assert_eq!(cache.query(0), None);
        assert_eq!(
            cache.query(1),
            Some(vec![(2, 2), (2, 5), (3, 6), (3, 7), (100, 8)])
        );
        assert_eq!(cache.query(2), Some(vec![(3, 6), (3, 7), (100, 8)]));
        assert_eq!(cache.query(3), Some(vec![(100, 8)]));
        assert_eq!(cache.query(4), Some(vec![(100, 8)]));
        assert_eq!(cache.query(100), Some(vec![]));
        assert_eq!(cache.query(1000), Some(vec![]));
    }

    #[test]
    fn query_at_capacity() {
        let mut cache = SequentialCache::<u32, u32>::new("query_at_capacity", 3);
        cache.insert(vec![(1, 1), (2, 2), (3, 3)]).unwrap();
        cache.insert(vec![(4, 4)]).unwrap();
        assert_eq!(cache.query(1), None);
        assert_eq!(cache.query(2), Some(vec![(3, 3), (4, 4)]));
    }

    #[test]
    fn insertion_at_capacity_limit() {
        let mut cache = SequentialCache::<u32, String>::new("insertion_at_capacity_limit", 2);
        cache
            .insert(vec![(1, "One".to_string()), (2, "Two".to_string())])
            .unwrap();

        // This should cause the first entry to be removed and only the last two to remain.
        cache
            .insert(vec![(3, "Three".to_string()), (4, "Four".to_string())])
            .unwrap();

        assert_eq!(cache.query(1), None);
        assert_eq!(cache.query(3), Some(vec![(4, "Four".to_string())]));
    }
}
