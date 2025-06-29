//! Tests for `PostgresStorage`.

use std::{collections::HashMap, mem, time::Duration};

use rand::{
    rngs::StdRng,
    seq::{IteratorRandom, SliceRandom},
    Rng, SeedableRng,
};
use test_casing::test_casing;
use zksync_dal::ConnectionPool;
use zksync_types::StorageLog;

use super::*;
use crate::test_utils::{create_l1_batch, create_l2_block, gen_storage_logs, prepare_postgres};

fn test_postgres_storage_basics(
    pool: &ConnectionPool<Core>,
    rt_handle: Handle,
    cache_initial_writes: bool,
) {
    let mut connection = rt_handle.block_on(pool.connection()).unwrap();
    rt_handle.block_on(prepare_postgres(&mut connection));
    let mut storage = PostgresStorage::new(rt_handle, connection, L2BlockNumber(0), true);
    if cache_initial_writes {
        let caches = PostgresStorageCaches::new(1_024, 1_024);
        storage = storage.with_caches(caches);
    }
    assert_eq!(storage.l1_batch_number_for_l2_block, L1BatchNumber(0));

    let existing_logs = gen_storage_logs(0..20);
    for log in &existing_logs {
        assert!(!storage.is_write_initial(&log.key));
    }

    let non_existing_logs = gen_storage_logs(20..30);
    for log in &non_existing_logs {
        assert!(storage.is_write_initial(&log.key));
    }

    if cache_initial_writes {
        let caches = storage.caches.as_ref().unwrap();
        assert!(caches.initial_writes.estimated_len() > 0);
    }

    // Add a new L2 block to the storage
    storage.rt_handle.block_on(create_l2_block(
        &mut storage.connection,
        L2BlockNumber(1),
        &non_existing_logs,
        L1BatchNumber(1),
    ));

    // Check that the L2 block is not seen by `PostgresStorage` (it's not a part of an L1 batch)
    for log in &non_existing_logs {
        assert!(storage.is_write_initial(&log.key));
    }

    let caches = mem::take(&mut storage.caches);
    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(1),
        true,
    );
    storage.caches = caches;

    assert_eq!(storage.l1_batch_number_for_l2_block, L1BatchNumber(1));
    for log in &non_existing_logs {
        assert!(storage.is_write_initial(&log.key));
    }

    // Create an L1 batch for L2 block #1
    storage.rt_handle.block_on(create_l1_batch(
        &mut storage.connection,
        L1BatchNumber(1),
        &non_existing_logs,
    ));

    // L2 block #1 should not be seen by the "old" storage
    let caches = mem::take(&mut storage.caches);
    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(0),
        true,
    );
    storage.caches = caches;

    assert_eq!(storage.l1_batch_number_for_l2_block, L1BatchNumber(0));
    for log in &non_existing_logs {
        assert!(storage.is_write_initial(&log.key));
    }

    // ...but should be seen by the new one
    let caches = mem::take(&mut storage.caches);
    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(1),
        true,
    );
    storage.caches = caches;

    assert_eq!(storage.l1_batch_number_for_l2_block, L1BatchNumber(1));
    for log in &non_existing_logs {
        assert!(!storage.is_write_initial(&log.key));
    }

    // ...except if we set `consider_new_l1_batch` to `false`
    let caches = mem::take(&mut storage.caches);
    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(1),
        false,
    );
    storage.caches = caches;

    assert_eq!(storage.l1_batch_number_for_l2_block, L1BatchNumber(1));
    for log in &non_existing_logs {
        assert!(storage.is_write_initial(&log.key));
    }
    for log in &existing_logs {
        assert!(!storage.is_write_initial(&log.key));
    }
}

#[tokio::test]
async fn postgres_storage_basics() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    tokio::task::spawn_blocking(move || {
        test_postgres_storage_basics(&pool, Handle::current(), false);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn postgres_storage_with_initial_writes_cache() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    tokio::task::spawn_blocking(move || {
        test_postgres_storage_basics(&pool, Handle::current(), true);
    })
    .await
    .unwrap();
}

fn test_postgres_storage_after_sealing_l2_block(
    pool: &ConnectionPool<Core>,
    rt_handle: Handle,
    consider_new_l1_batch: bool,
) {
    let mut connection = rt_handle.block_on(pool.connection()).unwrap();
    rt_handle.block_on(prepare_postgres(&mut connection));
    let new_logs = gen_storage_logs(20..30);

    rt_handle.block_on(create_l2_block(
        &mut connection,
        L2BlockNumber(1),
        &new_logs,
        L1BatchNumber(1),
    ));

    let mut storage = PostgresStorage::new(
        rt_handle,
        connection,
        L2BlockNumber(1),
        consider_new_l1_batch,
    );
    assert_eq!(storage.l1_batch_number_for_l2_block, L1BatchNumber(1));

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
        L2BlockNumber(1),
        consider_new_l1_batch,
    );
    assert_eq!(storage.l1_batch_number_for_l2_block, L1BatchNumber(1));
    for log in &new_logs {
        assert_eq!(storage.is_write_initial(&log.key), !consider_new_l1_batch);
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn postgres_storage_after_sealing_l2_block(consider_new_batch: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    tokio::task::spawn_blocking(move || {
        test_postgres_storage_after_sealing_l2_block(&pool, Handle::current(), consider_new_batch);
    })
    .await
    .unwrap();
}

fn test_factory_deps_cache(pool: &ConnectionPool<Core>, rt_handle: Handle) {
    let mut connection = rt_handle.block_on(pool.connection()).unwrap();
    rt_handle.block_on(prepare_postgres(&mut connection));

    let caches = PostgresStorageCaches::new(128 * 1_024 * 1_024, 1_024);
    let mut storage = PostgresStorage::new(rt_handle, connection, L2BlockNumber(1), true)
        .with_caches(caches.clone());

    let zero_addr = H256::zero();
    // try load a non-existent contract
    let dep = storage.load_factory_dep(zero_addr);

    assert_eq!(dep, None);
    assert_eq!(caches.factory_deps.get(&zero_addr), None);

    // insert the contracts
    let mut contracts = HashMap::new();
    contracts.insert(H256::zero(), vec![1, 2, 3]);
    storage
        .rt_handle
        .block_on(
            storage
                .connection
                .factory_deps_dal()
                .insert_factory_deps(L2BlockNumber(0), &contracts),
        )
        .unwrap();

    let mut contracts = HashMap::new();
    contracts.insert(H256::from_low_u64_be(1), vec![1, 2, 3, 4]);
    storage
        .rt_handle
        .block_on(
            storage
                .connection
                .factory_deps_dal()
                .insert_factory_deps(L2BlockNumber(1), &contracts),
        )
        .unwrap();

    // Create the storage that should have the cache filled.
    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(1),
        true,
    )
    .with_caches(caches.clone());

    // Fill the cache
    let dep = storage.load_factory_dep(zero_addr);
    assert_eq!(dep, Some(vec![1, 2, 3]));
    assert_eq!(
        caches.factory_deps.get(&zero_addr),
        Some(TimestampedFactoryDep {
            bytecode: vec![1, 2, 3],
            inserted_at: L2BlockNumber(0)
        })
    );

    let dep = storage.load_factory_dep(H256::from_low_u64_be(1));
    assert_eq!(dep, Some(vec![1, 2, 3, 4]));
    assert_eq!(
        caches.factory_deps.get(&H256::from_low_u64_be(1)),
        Some(TimestampedFactoryDep {
            bytecode: vec![1, 2, 3, 4],
            inserted_at: L2BlockNumber(1)
        })
    );

    // Create storage with `L2BlockNumber(0)`.
    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(0),
        true,
    )
    .with_caches(caches.clone());

    // First bytecode was published at L2 block 0, so it should be visible.
    let dep = storage.load_factory_dep(zero_addr);
    assert_eq!(dep, Some(vec![1, 2, 3]));

    // Second bytecode was published at L2 block 1, so it shouldn't be visible.
    let dep = storage.load_factory_dep(H256::from_low_u64_be(1));
    assert!(dep.is_none());
}

#[tokio::test]
async fn using_factory_deps_cache() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let handle = Handle::current();
    tokio::task::spawn_blocking(move || test_factory_deps_cache(&pool, handle))
        .await
        .unwrap();
}

fn test_initial_writes_cache(pool: &ConnectionPool<Core>, rt_handle: Handle) {
    let connection = rt_handle.block_on(pool.connection()).unwrap();
    let caches = PostgresStorageCaches::new(1_024, 4 * 1_024 * 1_024);
    let mut storage = PostgresStorage::new(rt_handle, connection, L2BlockNumber(0), false)
        .with_caches(caches.clone());
    assert_eq!(storage.pending_l1_batch_number, L1BatchNumber(0));

    storage
        .rt_handle
        .block_on(prepare_postgres(&mut storage.connection));

    let mut logs = gen_storage_logs(100..120);
    let non_existing_key = logs[19].key;
    logs.truncate(10);

    assert!(storage.is_write_initial(&logs[0].key));
    assert!(storage.is_write_initial(&non_existing_key));
    assert_eq!(
        caches
            .negative_initial_writes
            .get(&logs[0].key.hashed_key()),
        Some(L1BatchNumber(0))
    );
    assert_eq!(
        caches
            .negative_initial_writes
            .get(&non_existing_key.hashed_key()),
        Some(L1BatchNumber(0))
    );
    assert!(storage.is_write_initial(&logs[0].key));
    assert!(storage.is_write_initial(&non_existing_key));

    storage.rt_handle.block_on(create_l2_block(
        &mut storage.connection,
        L2BlockNumber(1),
        &logs,
        L1BatchNumber(1),
    ));
    storage.rt_handle.block_on(create_l1_batch(
        &mut storage.connection,
        L1BatchNumber(1),
        &logs,
    ));

    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(1),
        false,
    )
    .with_caches(caches.clone());

    assert!(storage.is_write_initial(&logs[0].key));
    // ^ Since we don't consider the latest L1 batch
    assert!(storage.is_write_initial(&non_existing_key));

    // Check that the cache entries have been updated
    assert_eq!(
        caches.initial_writes.get(&logs[0].key.hashed_key()),
        Some(L1BatchNumber(1))
    );
    assert_eq!(
        caches
            .negative_initial_writes
            .get(&logs[0].key.hashed_key()),
        None
    );
    assert_eq!(
        caches
            .negative_initial_writes
            .get(&non_existing_key.hashed_key()),
        Some(L1BatchNumber(2))
    );
    assert!(storage.is_write_initial(&logs[0].key));
    assert!(storage.is_write_initial(&non_existing_key));

    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(1),
        true,
    )
    .with_caches(caches.clone());
    assert!(!storage.is_write_initial(&logs[0].key));
    assert!(storage.is_write_initial(&non_existing_key));

    // Check that the cache entries are still as expected.
    assert_eq!(
        caches.initial_writes.get(&logs[0].key.hashed_key()),
        Some(L1BatchNumber(1))
    );
    assert_eq!(
        caches
            .negative_initial_writes
            .get(&non_existing_key.hashed_key()),
        Some(L1BatchNumber(2))
    );

    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(2),
        false,
    )
    .with_caches(caches);

    // Check that the cached value has been used
    assert!(!storage.is_write_initial(&logs[0].key));
    assert!(storage.is_write_initial(&non_existing_key));
}

#[tokio::test]
async fn using_initial_writes_cache() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let handle = Handle::current();
    tokio::task::spawn_blocking(move || test_initial_writes_cache(&pool, handle))
        .await
        .unwrap();
}

#[derive(Debug)]
struct ValueCacheAssertions<'a> {
    cache: &'a ValuesCache,
    l2_block_number: L2BlockNumber,
}

impl ValueCacheAssertions<'_> {
    fn assert_entries(&self, expected_entries: &[(StorageKey, Option<StorageValue>)]) {
        for (key, expected_value) in expected_entries {
            assert_eq!(
                self.cache.get(self.l2_block_number, key.hashed_key()),
                *expected_value
            );
        }
    }
}

impl ValuesCache {
    fn assertions(&self, l2_block_number: L2BlockNumber) -> ValueCacheAssertions<'_> {
        ValueCacheAssertions {
            cache: self,
            l2_block_number,
        }
    }
}

async fn wait_for_cache_update(values_cache: &ValuesCache, target_l2_block: L2BlockNumber) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let valid_for = values_cache.0.read().unwrap().valid_for;
            assert!(valid_for <= target_l2_block, "{valid_for:?}");
            if valid_for == target_l2_block {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("timed out waiting for cache update");
}

fn test_values_cache(pool: &ConnectionPool<Core>, rt_handle: Handle) {
    let mut caches = PostgresStorageCaches::new(1_024, 1_024);
    let task = caches.configure_storage_values_cache(1_024 * 1_024, 5, pool.clone());
    let (stop_sender, stop_receiver) = watch::channel(false);
    let update_task_handle = tokio::task::spawn(task.run(stop_receiver));

    let values_cache = caches.values.as_ref().unwrap().cache.clone();
    let old_l2_block_assertions = values_cache.assertions(L2BlockNumber(0));
    let new_l2_block_assertions = values_cache.assertions(L2BlockNumber(1));

    let mut connection = rt_handle.block_on(pool.connection()).unwrap();
    rt_handle.block_on(prepare_postgres(&mut connection));

    let mut storage = PostgresStorage::new(rt_handle, connection, L2BlockNumber(0), false)
        .with_caches(caches.clone());

    let initial_logs = gen_storage_logs(0..20);
    let existing_key = initial_logs[1].key;
    let unmodified_key = initial_logs[2].key;
    let initial_value = storage.read_value(&existing_key);
    assert!(!initial_value.is_zero());
    let unmodified_value = storage.read_value(&unmodified_key);
    assert!(!unmodified_value.is_zero());
    let non_existing_key = gen_storage_logs(100..120)[0].key;
    let value = storage.read_value(&non_existing_key);
    assert_eq!(value, StorageValue::zero());

    // Check that the read values are now cached.
    old_l2_block_assertions.assert_entries(&[
        (existing_key, Some(initial_value)),
        (unmodified_key, Some(unmodified_value)),
        (non_existing_key, Some(H256::zero())),
    ]);

    let logs = vec![
        StorageLog::new_write_log(existing_key, H256::repeat_byte(1)),
        StorageLog::new_write_log(non_existing_key, H256::repeat_byte(2)),
    ];
    storage.rt_handle.block_on(create_l2_block(
        &mut storage.connection,
        L2BlockNumber(1),
        &logs,
        L1BatchNumber(1),
    ));

    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(1),
        true,
    )
    .with_caches(caches.clone());

    // Cached values should not be updated so far, and they should not be used
    assert_eq!(storage.read_value(&existing_key), H256::repeat_byte(1));
    assert_eq!(storage.read_value(&non_existing_key), H256::repeat_byte(2));
    assert_eq!(storage.read_value(&unmodified_key), unmodified_value);

    new_l2_block_assertions.assert_entries(&[
        (existing_key, None),
        (unmodified_key, None),
        (non_existing_key, None),
    ]);
    // ^ We don't know at this point whether any keys are updated or not for L2 block #1
    old_l2_block_assertions.assert_entries(&[
        (existing_key, Some(initial_value)),
        (non_existing_key, Some(H256::zero())),
    ]);

    caches.schedule_values_update(L2BlockNumber(1));
    storage
        .rt_handle
        .block_on(wait_for_cache_update(&values_cache, L2BlockNumber(1)));

    assert_eq!(storage.read_value(&existing_key), H256::repeat_byte(1));
    assert_eq!(storage.read_value(&non_existing_key), H256::repeat_byte(2));
    assert_eq!(storage.read_value(&unmodified_key), unmodified_value);

    let assert_final_cache = || {
        // Check that the values are now cached.
        new_l2_block_assertions.assert_entries(&[
            (existing_key, Some(H256::repeat_byte(1))),
            (non_existing_key, Some(H256::repeat_byte(2))),
            (unmodified_key, Some(unmodified_value)),
        ]);
        // Check that the value for `unmodified_key` (and only for it) is used for `L2BlockNumber(0)`.
        old_l2_block_assertions.assert_entries(&[
            (existing_key, None),
            (non_existing_key, None),
            (unmodified_key, Some(unmodified_value)),
        ]);
    };
    assert_final_cache();

    let mut storage = PostgresStorage::new(
        storage.rt_handle,
        storage.connection,
        L2BlockNumber(0),
        true,
    )
    .with_caches(caches.clone());

    assert_eq!(storage.read_value(&existing_key), initial_value);
    assert_eq!(storage.read_value(&non_existing_key), StorageValue::zero());
    assert_eq!(storage.read_value(&unmodified_key), unmodified_value);

    // None of the cache entries should be modified.
    assert_final_cache();

    stop_sender.send_replace(true);
    storage
        .rt_handle
        .block_on(update_task_handle)
        .expect("update task panicked")
        .unwrap();
    // Check that `schedule_values_update()` doesn't panic after the update task is finished.
    caches.schedule_values_update(L2BlockNumber(2));
}

#[tokio::test]
async fn using_values_cache() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let handle = Handle::current();
    tokio::task::spawn_blocking(move || test_values_cache(&pool, handle))
        .await
        .unwrap();
}

/// (Sort of) fuzzes [`ValuesCache`] by comparing outputs of [`PostgresStorage`] with and without caching
/// on randomly generated `read_value()` queries.
fn mini_fuzz_values_cache_inner(
    rng: &mut impl Rng,
    pool: &ConnectionPool<Core>,
    mut rt_handle: Handle,
) {
    let mut caches = PostgresStorageCaches::new(1_024, 1_024);
    let _ = caches.configure_storage_values_cache(1_024 * 1_024, 5, pool.clone());
    let values_cache = caches.values.as_ref().unwrap().cache.clone();

    let mut connection = rt_handle.block_on(pool.connection()).unwrap();
    rt_handle.block_on(prepare_postgres(&mut connection));

    let queried_keys: Vec<_> = gen_storage_logs(0..100)
        .into_iter()
        .map(|log| log.key)
        .collect();

    for latest_block_number in 0..=10 {
        let mut all_block_numbers: Vec<_> = (0..=latest_block_number).map(L2BlockNumber).collect();
        all_block_numbers.shuffle(rng);

        let mut cache_updated = latest_block_number == 0;

        // Check outputs for all possible `block_number` arguments in the random order.
        for block_number in all_block_numbers {
            // Emulate updating cache with a delay after a new L2 block is sealed. `PostgresStorage`
            // must work both with and without the update.
            if !cache_updated && rng.gen_range(0..3) == 0 {
                let cache_valid_for = values_cache.valid_for();
                assert!(cache_valid_for < L2BlockNumber(latest_block_number));

                rt_handle
                    .block_on(values_cache.update(
                        cache_valid_for,
                        L2BlockNumber(latest_block_number),
                        Duration::ZERO,
                        &mut connection,
                    ))
                    .unwrap();
                cache_updated = true;
            }

            let mut queried_keys = queried_keys.clone();
            queried_keys.shuffle(rng);

            let mut uncached_storage =
                PostgresStorage::new(rt_handle, connection, block_number, false);
            let uncached_storage_output: Vec<_> = queried_keys
                .iter()
                .map(|key| uncached_storage.read_value(key))
                .collect();
            rt_handle = uncached_storage.rt_handle;
            connection = uncached_storage.connection;

            let mut cached_storage =
                PostgresStorage::new(rt_handle, connection, block_number, false)
                    .with_caches(caches.clone());
            let cached_storage_output: Vec<_> = queried_keys
                .iter()
                .map(|key| cached_storage.read_value(key))
                .collect();
            rt_handle = cached_storage.rt_handle;
            connection = cached_storage.connection;

            assert_eq!(
                uncached_storage_output, cached_storage_output,
                "Outputs differ for {block_number:?} with latest {latest_block_number:?}"
            );
        }

        let next_block_number = L2BlockNumber(latest_block_number) + 1;
        // Choose logs so that there's a chance that some of them are new and some overwrite previous values.
        let logs: Vec<_> = queried_keys
            .iter()
            .choose_multiple(rng, 20)
            .into_iter()
            .map(|&key| {
                let new_value = H256::from_low_u64_be(next_block_number.0.into());
                StorageLog::new_write_log(key, new_value)
            })
            .collect();
        rt_handle.block_on(create_l2_block(
            &mut connection,
            next_block_number,
            &logs,
            L1BatchNumber(1),
        ));
    }
}

#[tokio::test]
async fn mini_fuzz_values_cache() {
    const RNG_SEED: u64 = 123;
    let pool = ConnectionPool::<Core>::test_pool().await;

    let handle = Handle::current();
    let mut rng = StdRng::seed_from_u64(RNG_SEED);
    tokio::task::spawn_blocking(move || mini_fuzz_values_cache_inner(&mut rng, &pool, handle))
        .await
        .unwrap();
}
