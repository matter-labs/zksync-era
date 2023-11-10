use std::collections::HashMap;

use zksync_utils::u256_to_h256;

use crate::tx::tx_execution_info::DeduplicatedWritesMetrics;
use crate::writes::compression::compress_with_best_strategy;
use crate::{AccountTreeId, StorageKey, StorageLogQuery, StorageLogQueryType, U256};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ModifiedSlot {
    /// Value of the slot after modification.
    pub value: U256,
    /// Index (in L1 batch) of the transaction that lastly modified the slot.
    pub tx_index: u16,
    /// Size of pubdata update in bytes
    pub size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum UpdateType {
    Remove(ModifiedSlot),
    Update(ModifiedSlot),
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct UpdateItem {
    key: StorageKey,
    update_type: UpdateType,
    is_write_initial: bool,
}

/// Struct that allows to deduplicate storage writes in-flight.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StorageWritesDeduplicator {
    initial_values: HashMap<StorageKey, U256>,
    // stores the mapping of storage-slot key to its values and the tx number in block
    modified_key_values: HashMap<StorageKey, ModifiedSlot>,
    metrics: DeduplicatedWritesMetrics,
}

impl StorageWritesDeduplicator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn metrics(&self) -> DeduplicatedWritesMetrics {
        self.metrics
    }

    pub fn into_modified_key_values(self) -> HashMap<StorageKey, ModifiedSlot> {
        self.modified_key_values
    }

    /// Applies storage logs to the state.
    pub fn apply<'a, I: IntoIterator<Item = &'a StorageLogQuery>>(&mut self, logs: I) {
        self.process_storage_logs(logs);
    }

    /// Returns metrics as if provided storage logs are applied to the state.
    /// It's implemented in the following way: apply logs -> save current metrics -> rollback logs.
    pub fn apply_and_rollback<'a, I: IntoIterator<Item = &'a StorageLogQuery>>(
        &mut self,
        logs: I,
    ) -> DeduplicatedWritesMetrics {
        let updates = self.process_storage_logs(logs);
        let metrics = self.metrics;
        self.rollback(updates);
        metrics
    }

    /// Applies logs to the empty state and returns metrics.
    pub fn apply_on_empty_state<'a, I: IntoIterator<Item = &'a StorageLogQuery>>(
        logs: I,
    ) -> DeduplicatedWritesMetrics {
        let mut deduplicator = Self::new();
        deduplicator.apply(logs);
        deduplicator.metrics
    }

    /// Processes storage logs and returns updates for `modified_keys` and `metrics` fields.
    /// Metrics can be used later to rollback the state.
    /// We don't care about `initial_values` changes as we only inserted values there and they are always valid.
    fn process_storage_logs<'a, I: IntoIterator<Item = &'a StorageLogQuery>>(
        &mut self,
        logs: I,
    ) -> Vec<UpdateItem> {
        let mut updates = Vec::new();
        for log in logs.into_iter().filter(|log| log.log_query.rw_flag) {
            let key = StorageKey::new(
                AccountTreeId::new(log.log_query.address),
                u256_to_h256(log.log_query.key),
            );
            let initial_value = *self
                .initial_values
                .entry(key)
                .or_insert(log.log_query.read_value);
            let was_key_modified = self.modified_key_values.get(&key).is_some();
            let modified_value = if log.log_query.rollback {
                (initial_value != log.log_query.read_value).then_some(log.log_query.read_value)
            } else {
                (initial_value != log.log_query.written_value)
                    .then_some(log.log_query.written_value)
            };

            let is_write_initial = log.log_type == StorageLogQueryType::InitialWrite;
            let field_to_change = if is_write_initial {
                &mut self.metrics.initial_storage_writes
            } else {
                &mut self.metrics.repeated_storage_writes
            };

            let total_size = &mut self.metrics.total_updated_values_size;

            match (was_key_modified, modified_value) {
                (true, None) => {
                    let value = self.modified_key_values.remove(&key).unwrap_or_else(|| {
                        panic!("tried removing key: {:?} before insertion", key)
                    });
                    *field_to_change -= 1;
                    *total_size -= value.size;
                    updates.push(UpdateItem {
                        key,
                        update_type: UpdateType::Remove(value),
                        is_write_initial,
                    });
                }
                (true, Some(new_value)) => {
                    let value_size = compress_with_best_strategy(initial_value, new_value).len();
                    let old_value = self
                        .modified_key_values
                        .insert(
                            key,
                            ModifiedSlot {
                                value: new_value,
                                tx_index: log.log_query.tx_number_in_block,
                                size: value_size,
                            },
                        )
                        .unwrap_or_else(|| {
                            panic!("tried removing key: {:?} before insertion", key)
                        });

                    updates.push(UpdateItem {
                        key,
                        update_type: UpdateType::Update(old_value),
                        is_write_initial,
                    });

                    *total_size -= old_value.size;
                    *total_size += value_size;
                }
                (false, Some(new_value)) => {
                    let value_size = compress_with_best_strategy(initial_value, new_value).len();
                    self.modified_key_values.insert(
                        key,
                        ModifiedSlot {
                            value: new_value,
                            tx_index: log.log_query.tx_number_in_block,
                            size: value_size,
                        },
                    );
                    *field_to_change += 1;
                    *total_size += value_size;
                    updates.push(UpdateItem {
                        key,
                        update_type: UpdateType::Insert,
                        is_write_initial,
                    });
                }
                _ => {}
            }
        }
        updates
    }

    fn rollback(&mut self, updates: Vec<UpdateItem>) {
        for item in updates.into_iter().rev() {
            let field_to_change = if item.is_write_initial {
                &mut self.metrics.initial_storage_writes
            } else {
                &mut self.metrics.repeated_storage_writes
            };

            let total_size = &mut self.metrics.total_updated_values_size;

            match item.update_type {
                UpdateType::Insert => {
                    let value = self
                        .modified_key_values
                        .remove(&item.key)
                        .unwrap_or_else(|| {
                            panic!("tried removing key: {:?} before insertion", item.key)
                        });
                    *field_to_change -= 1;
                    *total_size -= value.size;
                }
                UpdateType::Update(value) => {
                    let old_value = self
                        .modified_key_values
                        .insert(item.key, value)
                        .unwrap_or_else(|| {
                            panic!("tried removing key: {:?} before insertion", item.key)
                        });
                    *total_size += value.size;
                    *total_size -= old_value.size;
                }
                UpdateType::Remove(value) => {
                    self.modified_key_values.insert(item.key, value);
                    *field_to_change += 1;
                    *total_size += value.size;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zk_evm::aux_structures::{LogQuery, Timestamp};

    use crate::H160;

    use super::*;

    fn storage_log_query(
        key: U256,
        read_value: U256,
        written_value: U256,
        rollback: bool,
        is_initial: bool,
    ) -> StorageLogQuery {
        let log_type = if is_initial {
            StorageLogQueryType::InitialWrite
        } else {
            StorageLogQueryType::RepeatedWrite
        };
        StorageLogQuery {
            log_query: LogQuery {
                timestamp: Timestamp(0),
                tx_number_in_block: 0,
                aux_byte: 0,
                shard_id: 0,
                address: Default::default(),
                key,
                read_value,
                written_value,
                rw_flag: true,
                rollback,
                is_service: false,
            },
            log_type,
        }
    }

    fn storage_log_query_with_address(
        address: H160,
        key: U256,
        written_value: U256,
    ) -> StorageLogQuery {
        let mut log = storage_log_query(key, 1234u32.into(), written_value, false, false);
        log.log_query.address = address;
        log
    }

    #[test]
    fn storage_writes_deduplicator() {
        // Each test scenario is a tuple (input, expected output, description).
        let scenarios: Vec<(Vec<StorageLogQuery>, DeduplicatedWritesMetrics, String)> = vec![
            (
                vec![storage_log_query(
                    0u32.into(),
                    0u32.into(),
                    1u32.into(),
                    false,
                    true,
                )],
                DeduplicatedWritesMetrics {
                    initial_storage_writes: 1,
                    repeated_storage_writes: 0,
                    total_updated_values_size: 2,
                },
                "single initial write".into(),
            ),
            (
                vec![
                    storage_log_query(0u32.into(), 0u32.into(), 1u32.into(), false, true),
                    storage_log_query(1u32.into(), 0u32.into(), 1u32.into(), false, false),
                ],
                DeduplicatedWritesMetrics {
                    initial_storage_writes: 1,
                    repeated_storage_writes: 1,
                    total_updated_values_size: 4,
                },
                "initial and repeated write".into(),
            ),
            (
                vec![
                    storage_log_query(0u32.into(), 0u32.into(), 1u32.into(), false, true),
                    storage_log_query(0u32.into(), 0u32.into(), 1u32.into(), true, true),
                ],
                DeduplicatedWritesMetrics {
                    initial_storage_writes: 0,
                    repeated_storage_writes: 0,
                    total_updated_values_size: 0,
                },
                "single rollback".into(),
            ),
            (
                vec![storage_log_query(
                    0u32.into(),
                    10u32.into(),
                    10u32.into(),
                    false,
                    true,
                )],
                DeduplicatedWritesMetrics {
                    initial_storage_writes: 0,
                    repeated_storage_writes: 0,
                    total_updated_values_size: 0,
                },
                "idle write".into(),
            ),
            (
                vec![
                    storage_log_query(0u32.into(), 0u32.into(), 1u32.into(), false, true),
                    storage_log_query(0u32.into(), 1u32.into(), 2u32.into(), false, true),
                    storage_log_query(0u32.into(), 2u32.into(), 0u32.into(), false, true),
                ],
                DeduplicatedWritesMetrics {
                    initial_storage_writes: 0,
                    repeated_storage_writes: 0,
                    total_updated_values_size: 0,
                },
                "idle write cycle".into(),
            ),
            (
                vec![
                    storage_log_query(0u32.into(), 5u32.into(), 10u32.into(), false, true),
                    storage_log_query(1u32.into(), 1u32.into(), 2u32.into(), false, true),
                    storage_log_query(0u32.into(), 10u32.into(), 11u32.into(), false, true),
                    storage_log_query(0u32.into(), 10u32.into(), 11u32.into(), true, true),
                    storage_log_query(2u32.into(), 0u32.into(), 10u32.into(), false, false),
                    storage_log_query(2u32.into(), 10u32.into(), 0u32.into(), false, false),
                    storage_log_query(2u32.into(), 0u32.into(), 10u32.into(), false, false),
                ],
                DeduplicatedWritesMetrics {
                    initial_storage_writes: 2,
                    repeated_storage_writes: 1,
                    total_updated_values_size: 6,
                },
                "complex".into(),
            ),
        ];

        for (input, expected_metrics, descr) in scenarios {
            let actual_metrics = StorageWritesDeduplicator::apply_on_empty_state(&input);
            assert_eq!(
                actual_metrics, expected_metrics,
                "test scenario failed: {}",
                descr
            );

            // Check that `apply_and_rollback` works correctly.
            let mut deduplicator = StorageWritesDeduplicator::new();
            let metrics_after_application = deduplicator.apply_and_rollback(&input);
            assert_eq!(
                metrics_after_application, expected_metrics,
                "test scenario failed for `apply_and_rollback`: {}",
                descr
            );

            assert_eq!(
                deduplicator.metrics,
                Default::default(),
                "rolled back incorrectly for scenario: {:?}",
                descr
            )
        }
    }

    fn new_storage_key(address: u64, key: u32) -> StorageKey {
        StorageKey::new(
            AccountTreeId::new(H160::from_low_u64_be(address)),
            u256_to_h256(key.into()),
        )
    }

    #[test]
    fn test_no_duplicate_storage_logs() {
        let expected = HashMap::from([
            (
                new_storage_key(1, 5),
                ModifiedSlot {
                    value: 8u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
            (
                new_storage_key(1, 4),
                ModifiedSlot {
                    value: 6u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
            (
                new_storage_key(2, 5),
                ModifiedSlot {
                    value: 9u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
            (
                new_storage_key(2, 4),
                ModifiedSlot {
                    value: 11u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
            (
                new_storage_key(3, 5),
                ModifiedSlot {
                    value: 2u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
            (
                new_storage_key(3, 4),
                ModifiedSlot {
                    value: 7u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
        ]);
        let mut deduplicator = StorageWritesDeduplicator::new();
        let logs = [
            storage_log_query_with_address(H160::from_low_u64_be(1), 5u32.into(), 8u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(1), 4u32.into(), 6u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(2), 4u32.into(), 11u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(2), 5u32.into(), 9u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(3), 4u32.into(), 7u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(3), 5u32.into(), 2u32.into()),
        ];
        deduplicator.apply(&logs);
        assert_eq!(expected, deduplicator.modified_key_values);
    }

    #[test]
    fn test_duplicate_storage_logs_within_same_address() {
        let expected = HashMap::from([
            (
                new_storage_key(1, 5),
                ModifiedSlot {
                    value: 6u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
            (
                new_storage_key(2, 4),
                ModifiedSlot {
                    value: 11u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
            (
                new_storage_key(3, 6),
                ModifiedSlot {
                    value: 7u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
        ]);
        let mut deduplicator = StorageWritesDeduplicator::new();
        let logs = [
            storage_log_query_with_address(H160::from_low_u64_be(1), 5u32.into(), 8u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(1), 5u32.into(), 6u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(2), 4u32.into(), 9u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(2), 4u32.into(), 11u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(3), 6u32.into(), 2u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(3), 6u32.into(), 7u32.into()),
        ];
        deduplicator.apply(&logs);
        assert_eq!(expected, deduplicator.modified_key_values);
    }

    #[test]
    fn test_duplicate_all_storage_logs() {
        let expected = HashMap::from([
            (
                new_storage_key(1, 2),
                ModifiedSlot {
                    value: 3u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
            (
                new_storage_key(1, 2),
                ModifiedSlot {
                    value: 4u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
            (
                new_storage_key(1, 2),
                ModifiedSlot {
                    value: 5u32.into(),
                    tx_index: 0,
                    size: 2,
                },
            ),
        ]);
        let mut deduplicator = StorageWritesDeduplicator::new();
        let logs = [
            storage_log_query_with_address(H160::from_low_u64_be(1), 2u32.into(), 3u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(1), 2u32.into(), 4u32.into()),
            storage_log_query_with_address(H160::from_low_u64_be(1), 2u32.into(), 5u32.into()),
        ];
        deduplicator.apply(&logs);
        assert_eq!(expected, deduplicator.modified_key_values);
    }

    #[test]
    fn test_last_rollback() {
        let expected = HashMap::from([(
            new_storage_key(0, 1),
            ModifiedSlot {
                value: 2u32.into(),
                tx_index: 0,
                size: 2,
            },
        )]);
        let mut deduplicator = StorageWritesDeduplicator::new();
        let logs = [
            storage_log_query(
                U256::from(1u32),
                U256::from(1u32),
                U256::from(2u32),
                false,
                false,
            ),
            storage_log_query(
                U256::from(1u32),
                U256::from(2u32),
                U256::from(1u32),
                false,
                false,
            ),
            storage_log_query(
                U256::from(1u32),
                U256::from(2u32),
                U256::from(1u32),
                true,
                false,
            ),
        ];
        deduplicator.apply(&logs);
        assert_eq!(expected, deduplicator.modified_key_values);
    }
}
