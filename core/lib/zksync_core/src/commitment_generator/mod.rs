use std::time::Duration;

use anyhow::Context;
use itertools::Itertools;
use metrics::{CommitmentStage, METRICS};
use multivm::zk_evm_latest::ethereum_types::U256;
use tokio::{sync::watch, task::JoinHandle};
use zksync_commitment_utils::{bootloader_initial_content_commitment, events_queue_commitment};
use zksync_dal::ConnectionPool;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{
    commitment::{AuxCommitments, CommitmentCommonInput, CommitmentInput, L1BatchCommitment},
    writes::{InitialStorageWrite, RepeatedStorageWrite, StateDiffRecord},
    L1BatchNumber, ProtocolVersionId, StorageKey, H256,
};
use zksync_utils::h256_to_u256;

mod metrics;

const SLEEP_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct CommitmentGenerator {
    connection_pool: ConnectionPool,
    health_updater: HealthUpdater,
}

impl CommitmentGenerator {
    pub fn new(connection_pool: ConnectionPool) -> Self {
        Self {
            connection_pool,
            health_updater: ReactiveHealthCheck::new("commitment_generator").1,
        }
    }

    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    async fn calculate_aux_commitments(
        &self,
        l1_batch_number: L1BatchNumber,
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<AuxCommitments> {
        let mut connection = self
            .connection_pool
            .access_storage_tagged("commitment_generator")
            .await?;
        let events_queue = connection
            .blocks_dal()
            .get_events_queue(l1_batch_number)
            .await?
            .context("Events queue is required for post-boojum batch")?;
        let initial_bootloader_contents = connection
            .blocks_dal()
            .get_initial_bootloader_heap(l1_batch_number)
            .await?
            .context("Bootloader initial heap is missing")?;
        drop(connection);

        let events_commitment_task: JoinHandle<anyhow::Result<H256>> =
            tokio::task::spawn_blocking(move || {
                let latency = METRICS.events_queue_commitment_latency.start();
                let events_queue_commitment =
                    events_queue_commitment(&events_queue, protocol_version)
                        .context("Events queue commitment is required for post-boojum batch")?;
                latency.observe();

                Ok(events_queue_commitment)
            });

        let bootloader_memory_commitment_task: JoinHandle<anyhow::Result<H256>> =
            tokio::task::spawn_blocking(move || {
                let latency = METRICS.bootloader_content_commitment_latency.start();
                let bootloader_initial_content_commitment = bootloader_initial_content_commitment(
                    &initial_bootloader_contents,
                    protocol_version,
                )
                .context("Bootloader content commitment is required for post-boojum batch")?;
                latency.observe();

                Ok(bootloader_initial_content_commitment)
            });

        let events_queue_commitment = events_commitment_task
            .await
            .context("`events_commitment_task` failed")??;
        let bootloader_initial_content_commitment = bootloader_memory_commitment_task
            .await
            .context("`bootloader_memory_commitment_task` failed")??;

        Ok(AuxCommitments {
            events_queue_commitment,
            bootloader_initial_content_commitment,
        })
    }

    async fn prepare_input(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<CommitmentInput> {
        let mut connection = self
            .connection_pool
            .access_storage_tagged("commitment_generator")
            .await?;
        let header = connection
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?
            .context("header is missing for batch")?;
        let tree_data = connection
            .blocks_dal()
            .get_l1_batch_tree_data(l1_batch_number)
            .await?
            .context("tree data is missing for batch")?;

        // TODO(PLA-731): ensure that the protocol version is always available.
        let protocol_version = header
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);
        let common = CommitmentCommonInput {
            l2_to_l1_logs: header.l2_to_l1_logs,
            rollup_last_leaf_index: tree_data.rollup_last_leaf_index,
            rollup_root_hash: tree_data.hash,
            bootloader_code_hash: header.base_system_contracts_hashes.bootloader,
            default_aa_code_hash: header.base_system_contracts_hashes.default_aa,
            protocol_version,
        };
        let touched_slots = connection
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(l1_batch_number)
            .await?;
        let touched_hashed_keys: Vec<_> =
            touched_slots.keys().map(|key| key.hashed_key()).collect();
        let previous_values = connection
            .storage_logs_dal()
            .get_previous_storage_values(&touched_hashed_keys, l1_batch_number)
            .await?;
        let l1_batches_for_initial_writes = connection
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&touched_hashed_keys)
            .await?;
        drop(connection);

        let input = if protocol_version.is_pre_boojum() {
            let mut initial_writes = Vec::new();
            let mut repeated_writes = Vec::new();
            for (key, value) in touched_slots.into_iter().sorted_by_key(|(key, _)| *key) {
                let hashed_key = key.hashed_key();
                if previous_values[&hashed_key].unwrap_or_default() != value {
                    let (initial_write_l1_batch_number, index) =
                        l1_batches_for_initial_writes[&hashed_key];
                    assert!(
                        initial_write_l1_batch_number <= l1_batch_number,
                        "Slot {hashed_key:?} was changed in L1 batch {l1_batch_number} but in DB L1 batch of initial write is greater"
                    );
                    if initial_write_l1_batch_number == l1_batch_number {
                        initial_writes.push(InitialStorageWrite {
                            index,
                            key: key.hashed_key_u256(),
                            value,
                        });
                    } else {
                        repeated_writes.push(RepeatedStorageWrite { index, value });
                    }
                }
            }

            CommitmentInput::PreBoojum {
                common,
                initial_writes,
                repeated_writes,
            }
        } else {
            let aux_commitments = self
                .calculate_aux_commitments(header.number, protocol_version)
                .await?;

            let mut state_diffs = Vec::new();
            for (key, value) in touched_slots {
                let hashed_key = key.hashed_key();
                let prev_value = previous_values[&hashed_key].unwrap_or_default();
                if prev_value != value {
                    let (initial_write_l1_batch_number, index) =
                        l1_batches_for_initial_writes[&hashed_key];
                    assert!(
                        initial_write_l1_batch_number <= l1_batch_number,
                        "Slot {hashed_key:?} was changed in L1 batch {l1_batch_number} but in DB L1 batch of initial write is greater"
                    );
                    if initial_write_l1_batch_number == l1_batch_number {
                        state_diffs.push(StateDiffRecord {
                            address: *key.address(),
                            key: h256_to_u256(*key.key()),
                            derived_key: StorageKey::raw_hashed_key(key.address(), key.key()),
                            enumeration_index: 0u64,
                            initial_value: U256::default(),
                            final_value: h256_to_u256(value),
                        });
                    } else {
                        state_diffs.push(StateDiffRecord {
                            address: *key.address(),
                            key: h256_to_u256(*key.key()),
                            derived_key: StorageKey::raw_hashed_key(key.address(), key.key()),
                            enumeration_index: index,
                            initial_value: h256_to_u256(prev_value),
                            final_value: h256_to_u256(value),
                        });
                    }
                }
            }
            state_diffs.sort_unstable_by_key(|rec| (rec.address, rec.key));

            CommitmentInput::PostBoojum {
                common,
                system_logs: header.system_logs,
                state_diffs,
                aux_commitments,
            }
        };

        Ok(input)
    }

    async fn step(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<()> {
        let latency =
            METRICS.generate_commitment_latency_stage[&CommitmentStage::PrepareInput].start();
        let input = self.prepare_input(l1_batch_number).await?;
        let latency = latency.observe();
        tracing::debug!("Prepared commitment input for L1 batch #{l1_batch_number} in {latency:?}");

        let latency =
            METRICS.generate_commitment_latency_stage[&CommitmentStage::Calculate].start();
        let commitment = L1BatchCommitment::new(input);
        let artifacts = commitment.artifacts();
        let latency = latency.observe();
        tracing::debug!(
            "Generated commitment artifacts for L1 batch #{l1_batch_number} in {latency:?}"
        );

        let latency =
            METRICS.generate_commitment_latency_stage[&CommitmentStage::SaveResults].start();
        self.connection_pool
            .access_storage_tagged("commitment_generator")
            .await?
            .blocks_dal()
            .save_l1_batch_commitment_artifacts(l1_batch_number, &artifacts)
            .await?;
        let latency = latency.observe();
        tracing::debug!(
            "Stored commitment artifacts for L1 batch #{l1_batch_number} in {latency:?}"
        );

        let health_details = serde_json::json!({
            "l1_batch_number": l1_batch_number,
        });
        self.health_updater
            .update(Health::from(HealthStatus::Ready).with_details(health_details));
        Ok(())
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.health_updater.update(HealthStatus::Ready.into());
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, commitment generator is shutting down");
                break;
            }

            let Some(l1_batch_number) = self
                .connection_pool
                .access_storage_tagged("commitment_generator")
                .await?
                .blocks_dal()
                .get_next_l1_batch_ready_for_commitment_generation()
                .await?
            else {
                tokio::time::sleep(SLEEP_INTERVAL).await;
                continue;
            };

            self.step(l1_batch_number).await?;
        }
        Ok(())
    }
}
