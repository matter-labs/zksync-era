use crate::tx::tx_execution_info::DeduplicatedWritesMetrics;
use crate::{AccountTreeId, StorageKey, StorageLogQuery, StorageLogQueryType, U256};
use std::collections::{HashMap, HashSet};
use zksync_utils::u256_to_h256;

#[derive(Debug, Clone, Copy, PartialEq)]
struct UpdateItem {
    key: StorageKey,
    is_insertion: bool,
    is_write_initial: bool,
}

/// Struct that allows to deduplicate storage writes in-flight.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StorageWritesDeduplicator {
    initial_values: HashMap<StorageKey, U256>,
    modified_keys: HashSet<StorageKey>,
    metrics: DeduplicatedWritesMetrics,
}

impl StorageWritesDeduplicator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn metrics(&self) -> DeduplicatedWritesMetrics {
        self.metrics
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

            let was_key_modified = self.modified_keys.get(&key).is_some();
            let is_key_modified = if log.log_query.rollback {
                initial_value != log.log_query.read_value
            } else {
                initial_value != log.log_query.written_value
            };

            let is_write_initial = log.log_type == StorageLogQueryType::InitialWrite;
            let field_to_change = if is_write_initial {
                &mut self.metrics.initial_storage_writes
            } else {
                &mut self.metrics.repeated_storage_writes
            };

            match (was_key_modified, is_key_modified) {
                (true, false) => {
                    self.modified_keys.remove(&key);
                    *field_to_change -= 1;
                    updates.push(UpdateItem {
                        key,
                        is_insertion: false,
                        is_write_initial,
                    });
                }
                (false, true) => {
                    self.modified_keys.insert(key);
                    *field_to_change += 1;
                    updates.push(UpdateItem {
                        key,
                        is_insertion: true,
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

            if item.is_insertion {
                self.modified_keys.remove(&item.key);
                *field_to_change -= 1;
            } else {
                self.modified_keys.insert(item.key);
                *field_to_change += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zk_evm::aux_structures::{LogQuery, Timestamp};

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
                "rolled back incorrectly for scenario: {}",
                descr
            )
        }
    }
}
