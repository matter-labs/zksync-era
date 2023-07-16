use tokio::{runtime::Handle, time::Instant};

use crate::cache::Cache;
use crate::ReadStorage;
use zksync_dal::StorageProcessor;
use zksync_types::{L1BatchNumber, MiniblockNumber, StorageKey, StorageValue, H256};

/// [`FactoryDepsCache`] type alias for smart contract source code cache
pub type FactoryDepsCache = Cache<H256>;

/// [`ReadStorage`] implementation backed by the Postgres database.
#[derive(Debug)]
pub struct PostgresStorage<'a> {
    rt_handle: Handle,
    connection: StorageProcessor<'a>,
    block_number: MiniblockNumber,
    l1_batch_number: L1BatchNumber,
    consider_new_l1_batch: bool,
    factory_deps_cache: Option<FactoryDepsCache>,
}

impl<'a> PostgresStorage<'a> {
    /// Creates a new storage using the specified connection.
    pub fn new(
        rt_handle: Handle,
        mut connection: StorageProcessor<'a>,
        block_number: MiniblockNumber,
        consider_new_l1_batch: bool,
    ) -> PostgresStorage<'a> {
        let mut dal = connection.storage_web3_dal();
        let l1_batch_number = rt_handle
            .block_on(dal.get_provisional_l1_batch_number_of_miniblock_unchecked(block_number))
            .expect("cannot fetch L1 batch number for miniblock");

        Self {
            rt_handle,
            connection,
            block_number,
            l1_batch_number,
            consider_new_l1_batch,
            factory_deps_cache: None,
        }
    }

    /// Sets the smart contract source code cache.
    #[must_use]
    pub fn with_factory_deps_cache(self, cache: FactoryDepsCache) -> Self {
        Self {
            factory_deps_cache: Some(cache),
            ..self
        }
    }
}

impl ReadStorage for PostgresStorage<'_> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let started_at = Instant::now();
        let mut dal = self.connection.storage_web3_dal();
        let value = self
            .rt_handle
            .block_on(async {
                metrics::histogram!(
                    "state.postgres_storage.enter_context",
                    started_at.elapsed(),
                    "method" => "read_value"
                );
                dal.get_historical_value_unchecked(key, self.block_number)
                    .await
            })
            .unwrap();
        metrics::histogram!("state.postgres_storage", started_at.elapsed(), "method" => "read_value");
        value
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let mut dal = self.connection.storage_web3_dal();
        let started_at = Instant::now();
        let initial_write_l1_batch_number = self
            .rt_handle
            .block_on(async {
                metrics::histogram!(
                    "state.postgres_storage.enter_context",
                    started_at.elapsed(),
                    "method" => "is_write_initial"
                );
                dal.get_l1_batch_number_for_initial_write(key).await
            })
            .unwrap();

        metrics::histogram!("state.postgres_storage", started_at.elapsed(), "method" => "is_write_initial");

        let contains_key =
            initial_write_l1_batch_number.map_or(false, |initial_write_l1_batch_number| {
                if self.consider_new_l1_batch {
                    self.l1_batch_number >= initial_write_l1_batch_number
                } else {
                    self.l1_batch_number > initial_write_l1_batch_number
                }
            });
        !contains_key
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        let started_at = Instant::now();
        let cached_value = self
            .factory_deps_cache
            .as_ref()
            .and_then(|cache| cache.get(&hash));
        let result = cached_value.or_else(|| {
            let mut dal = self.connection.storage_web3_dal();
            let value = self
                .rt_handle
                .block_on(async {
                    metrics::histogram!(
                        "state.postgres_storage.enter_context",
                        started_at.elapsed(),
                        "method" => "load_factory_dep"
                    );
                    dal.get_factory_dep_unchecked(hash, self.block_number).await
                })
                .unwrap();

            if let Some(cache) = &self.factory_deps_cache {
                // If we receive None, we won't cache it.
                if let Some(dep) = value.clone() {
                    cache.insert(hash, dep);
                }
            };

            value
        });

        metrics::histogram!(
            "state.postgres_storage",
            started_at.elapsed(),
            "method" => "load_factory_dep",
        );

        result
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use db_test_macro::db_test;
    use zksync_dal::ConnectionPool;

    use super::*;
    use crate::test_utils::{
        create_l1_batch, create_miniblock, gen_storage_logs, prepare_postgres,
    };

    fn test_postgres_storage_basics(pool: &ConnectionPool, rt_handle: Handle) {
        let mut connection = rt_handle.block_on(pool.access_storage());
        rt_handle.block_on(prepare_postgres(&mut connection));
        let mut storage = PostgresStorage::new(rt_handle, connection, MiniblockNumber(0), true);
        assert_eq!(storage.l1_batch_number, L1BatchNumber(0));

        let existing_logs = gen_storage_logs(0..20);
        for log in &existing_logs {
            assert!(!storage.is_write_initial(&log.key));
        }

        let non_existing_logs = gen_storage_logs(20..30);
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }

        // Add a new miniblock to the storage
        storage.rt_handle.block_on(create_miniblock(
            &mut storage.connection,
            MiniblockNumber(1),
            non_existing_logs.clone(),
        ));

        // Check that the miniblock is not seen by `PostgresStorage` (it's not a part of an L1 batch)
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }

        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            true,
        );
        assert_eq!(storage.l1_batch_number, L1BatchNumber(1));
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }

        // Create an L1 batch for miniblock #1
        storage.rt_handle.block_on(create_l1_batch(
            &mut storage.connection,
            L1BatchNumber(1),
            &non_existing_logs,
        ));

        // Miniblock #1 should not be seen by the "old" storage
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(0),
            true,
        );
        assert_eq!(storage.l1_batch_number, L1BatchNumber(0));
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }

        // ...but should be seen by the new one
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            true,
        );
        assert_eq!(storage.l1_batch_number, L1BatchNumber(1));
        for log in &non_existing_logs {
            assert!(!storage.is_write_initial(&log.key));
        }

        // ...except if we set `consider_new_l1_batch` to `false`
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            false,
        );
        assert_eq!(storage.l1_batch_number, L1BatchNumber(1));
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }
        for log in &existing_logs {
            assert!(!storage.is_write_initial(&log.key));
        }
    }

    #[db_test]
    async fn postgres_storage_basics(pool: ConnectionPool) {
        tokio::task::spawn_blocking(move || {
            test_postgres_storage_basics(&pool, Handle::current());
        })
        .await
        .unwrap();
    }

    fn test_postgres_storage_after_sealing_miniblock(
        pool: &ConnectionPool,
        rt_handle: Handle,
        consider_new_l1_batch: bool,
    ) {
        let mut connection = rt_handle.block_on(pool.access_storage());
        rt_handle.block_on(prepare_postgres(&mut connection));
        let new_logs = gen_storage_logs(20..30);

        rt_handle.block_on(create_miniblock(
            &mut connection,
            MiniblockNumber(1),
            new_logs.clone(),
        ));

        let mut storage = PostgresStorage::new(
            rt_handle,
            connection,
            MiniblockNumber(1),
            consider_new_l1_batch,
        );
        assert_eq!(storage.l1_batch_number, L1BatchNumber(1));

        storage.rt_handle.block_on(create_l1_batch(
            &mut storage.connection,
            L1BatchNumber(1),
            &new_logs,
        ));

        for log in &new_logs {
            assert_eq!(storage.is_write_initial(&log.key), !consider_new_l1_batch);
        }

        // Cross-check with the newly instantiated store with the same params
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            consider_new_l1_batch,
        );
        assert_eq!(storage.l1_batch_number, L1BatchNumber(1));
        for log in &new_logs {
            assert_eq!(storage.is_write_initial(&log.key), !consider_new_l1_batch);
        }
    }

    #[db_test]
    async fn postgres_storage_after_sealing_miniblock(pool: ConnectionPool) {
        tokio::task::spawn_blocking(move || {
            println!("Considering new L1 batch");
            test_postgres_storage_after_sealing_miniblock(&pool, Handle::current(), true);
            println!("Not considering new L1 batch");
            test_postgres_storage_after_sealing_miniblock(&pool, Handle::current(), false);
        })
        .await
        .unwrap();
    }

    fn test_postgres_storage_factory_deps_cache(
        pool: &ConnectionPool,
        rt_handle: &Handle,
        consider_new_l1_batch: bool,
    ) {
        let mut connection = rt_handle.block_on(pool.access_storage());
        rt_handle.block_on(prepare_postgres(&mut connection));
        let cache = FactoryDepsCache::new("test_factory_deps_cache", 128);
        let mut storage = PostgresStorage::new(
            rt_handle.clone(),
            connection,
            MiniblockNumber(1),
            consider_new_l1_batch,
        )
        .with_factory_deps_cache(cache.clone());

        let zero_addr = H256::zero();
        // try load a non-existent contract
        let dep = storage.load_factory_dep(zero_addr);

        assert_eq!(dep, None);
        assert_eq!(cache.get(&zero_addr.clone()), None);
        drop(storage); // Drop the storage to free the connection.

        // Prepare the new connection.
        let mut connection = rt_handle.block_on(pool.access_storage());
        // insert the contracts
        let mut contracts = HashMap::new();
        contracts.insert(H256::zero(), vec![1, 2, 3]);
        rt_handle.block_on(
            connection
                .storage_dal()
                .insert_factory_deps(MiniblockNumber(0), &contracts),
        );
        // Create the storage that should have the cache filled.
        let mut storage = PostgresStorage::new(
            rt_handle.clone(),
            connection,
            MiniblockNumber(1),
            consider_new_l1_batch,
        )
        .with_factory_deps_cache(cache.clone());

        // fill the cache
        let dep = storage.load_factory_dep(zero_addr);

        assert_eq!(dep, Some(vec![1, 2, 3]));
        assert_eq!(cache.get(&zero_addr.clone()), Some(vec![1, 2, 3]));
    }

    #[db_test]
    async fn postgres_storage_factory_deps_cache(pool: ConnectionPool) {
        let handle = Handle::current();
        tokio::task::spawn_blocking(move || {
            println!("Testing FactoryDepsCache integration");
            test_postgres_storage_factory_deps_cache(&pool, &handle, true);
        })
        .await
        .unwrap();
    }
}
