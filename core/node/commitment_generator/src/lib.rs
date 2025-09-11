use std::{num::NonZeroU32, ops, sync::Arc, time::Duration};

use anyhow::Context;
use itertools::Itertools;
use tokio::{sync::watch, task::JoinHandle};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_instrument::alloc::AllocationGuard;
use zksync_l1_contract_interface::i_executor::commit::kzg::pubdata_to_blob_commitments;
use zksync_multivm::zk_evm_latest::ethereum_types::U256;
use zksync_types::{
    blob::num_blobs_required,
    commitment::{
        AuxCommitments, BlobHash, CommitmentCommonInput, CommitmentInput, L1BatchAuxiliaryOutput,
        L1BatchCommitment, L1BatchCommitmentArtifacts, L1BatchCommitmentMode,
    },
    h256_to_u256,
    writes::{InitialStorageWrite, RepeatedStorageWrite, StateDiffRecord},
    L1BatchNumber, ProtocolVersionId, StorageKey, H256,
};

use crate::{
    metrics::{CommitmentStage, METRICS},
    utils::{
        convert_vm_events_to_log_queries, pubdata_to_blob_linear_hashes, read_aggregation_root,
        CommitmentComputer, RealCommitmentComputer,
    },
};

mod metrics;
pub mod node;
#[cfg(test)]
mod tests;
mod utils;
pub mod validation_task;

const SLEEP_INTERVAL: Duration = Duration::from_millis(100);

/// Component responsible for generating commitments for L1 batches.
#[derive(Debug)]
pub struct CommitmentGenerator {
    computer: Arc<dyn CommitmentComputer>,
    connection_pool: ConnectionPool<Core>,
    health_updater: HealthUpdater,
    parallelism: NonZeroU32,
}

impl CommitmentGenerator {
    /// Creates a commitment generator with the provided mode.
    pub fn new(connection_pool: ConnectionPool<Core>) -> Self {
        Self {
            computer: Arc::new(RealCommitmentComputer),
            connection_pool,
            health_updater: ReactiveHealthCheck::new("commitment_generator").1,
            parallelism: Self::default_parallelism(),
        }
    }

    /// Returns default parallelism for commitment generation based on the number of CPU cores available.
    pub fn default_parallelism() -> NonZeroU32 {
        // Leave at least one core free to handle other blocking tasks. `unwrap()`s are safe by design.
        let cpus = u32::try_from(num_cpus::get().saturating_sub(1).clamp(1, 16)).unwrap();
        NonZeroU32::new(cpus).unwrap()
    }

    /// Sets the degree of parallelism to be used by this generator. A reasonable value can be obtained
    /// using [`Self::default_parallelism()`].
    pub fn set_max_parallelism(&mut self, parallelism: NonZeroU32) {
        self.parallelism = parallelism;
    }

    /// Returns a health check for this generator.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    #[tracing::instrument(skip(self))]
    async fn calculate_aux_commitments(
        &self,
        l1_batch_number: L1BatchNumber,
        protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<AuxCommitments> {
        let mut connection = self
            .connection_pool
            .connection_tagged("commitment_generator")
            .await?;

        // Calculate events queue using VM events.
        let events_queue = {
            let events = connection
                .events_dal()
                .get_vm_events_for_l1_batch(l1_batch_number)
                .await?
                .with_context(|| format!("Events are missing for L1 batch #{l1_batch_number}"))?;
            convert_vm_events_to_log_queries(&events)
        };

        let initial_bootloader_contents = connection
            .blocks_dal()
            .get_initial_bootloader_heap(l1_batch_number)
            .await?
            .with_context(|| {
                format!("Bootloader initial heap is missing for L1 batch #{l1_batch_number}")
            })?;
        drop(connection);

        let computer = self.computer.clone();
        let span = tracing::Span::current();
        let events_commitment_task: JoinHandle<anyhow::Result<H256>> =
            tokio::task::spawn_blocking(move || {
                let _entered_span = span.entered();
                let _guard = AllocationGuard::for_operation("commitment_generator#events");
                let latency = METRICS.events_queue_commitment_latency.start();
                let events_queue_commitment =
                    computer.events_queue_commitment(&events_queue, protocol_version)?;
                latency.observe();

                Ok(events_queue_commitment)
            });

        let computer = self.computer.clone();
        let span = tracing::Span::current();
        let bootloader_memory_commitment_task: JoinHandle<anyhow::Result<H256>> =
            tokio::task::spawn_blocking(move || {
                let _entered_span = span.entered();
                let _guard =
                    AllocationGuard::for_operation("commitment_generator#bootloader_memory");
                let latency = METRICS.bootloader_content_commitment_latency.start();
                let bootloader_initial_content_commitment = computer
                    .bootloader_initial_content_commitment(
                        &initial_bootloader_contents,
                        protocol_version,
                    )?;
                latency.observe();

                Ok(bootloader_initial_content_commitment)
            });

        let events_queue_commitment = events_commitment_task.await.with_context(|| {
            format!("`events_commitment_task` failed for L1 batch #{l1_batch_number}")
        })??;
        let bootloader_initial_content_commitment =
            bootloader_memory_commitment_task.await.with_context(|| {
                format!(
                    "`bootloader_memory_commitment_task` failed for L1 batch #{l1_batch_number}"
                )
            })??;

        Ok(AuxCommitments {
            events_queue_commitment,
            bootloader_initial_content_commitment,
        })
    }

    #[tracing::instrument(skip(self))]
    async fn prepare_input(
        &self,
        l1_batch_number: L1BatchNumber,
        commitment_mode: L1BatchCommitmentMode,
    ) -> anyhow::Result<CommitmentInput> {
        tracing::info!("Started preparing commitment input for L1 batch #{l1_batch_number}");

        let mut connection = self
            .connection_pool
            .connection_tagged("commitment_generator")
            .await?;
        let header = connection
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await?
            .with_context(|| format!("header is missing for L1 batch #{l1_batch_number}"))?;
        let tree_data = connection
            .blocks_dal()
            .get_l1_batch_tree_data(l1_batch_number)
            .await?
            .with_context(|| format!("`tree_data` is missing for L1 batch #{l1_batch_number}"))?;

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
            evm_emulator_code_hash: header.base_system_contracts_hashes.evm_emulator,
            protocol_version,
        };
        let touched_slots = connection
            .storage_logs_dal()
            .get_touched_slots_for_executed_l1_batch(l1_batch_number)
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

        let mut input = if protocol_version.is_pre_boojum() {
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

            let blob_hashes = if protocol_version.is_post_1_4_2() {
                let pubdata_input = header.pubdata_input.with_context(|| {
                    format!("`pubdata_input` is missing for L1 batch #{l1_batch_number}")
                })?;

                let commitments = pubdata_to_blob_commitments(
                    num_blobs_required(&protocol_version),
                    &pubdata_input,
                );
                let linear_hashes = pubdata_to_blob_linear_hashes(
                    num_blobs_required(&protocol_version),
                    pubdata_input,
                );

                commitments
                    .into_iter()
                    .zip(linear_hashes)
                    .map(|(commitment, linear_hash)| BlobHash {
                        commitment,
                        linear_hash,
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![Default::default(); num_blobs_required(&protocol_version)]
            };

            let aggregation_root = if protocol_version.is_pre_gateway() {
                H256::zero()
            } else {
                let mut connection = self
                    .connection_pool
                    .connection_tagged("commitment_generator")
                    .await?;
                read_aggregation_root(&mut connection, l1_batch_number).await?
            };

            CommitmentInput::PostBoojum {
                common,
                system_logs: header.system_logs,
                state_diffs,
                aux_commitments,
                blob_hashes,
                aggregation_root,
            }
        };

        self.tweak_input(&mut input, commitment_mode);
        Ok(input)
    }

    #[tracing::instrument(skip(self))]
    async fn process_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<L1BatchCommitmentArtifacts> {
        let commitment_mode = self.get_commitment_mode(l1_batch_number).await?;

        let latency =
            METRICS.generate_commitment_latency_stage[&CommitmentStage::PrepareInput].start();
        let input = self.prepare_input(l1_batch_number, commitment_mode).await?;
        let latency = latency.observe();
        tracing::debug!("Prepared commitment input for L1 batch #{l1_batch_number} in {latency:?}");

        let latency =
            METRICS.generate_commitment_latency_stage[&CommitmentStage::Calculate].start();
        let mut commitment = L1BatchCommitment::new(input)?;
        self.post_process_commitment(&mut commitment, commitment_mode);
        let artifacts = commitment.artifacts()?;
        let latency = latency.observe();
        tracing::debug!(
            "Generated commitment artifacts for L1 batch #{l1_batch_number} in {latency:?}"
        );
        Ok(artifacts)
    }

    async fn get_commitment_mode(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<L1BatchCommitmentMode> {
        let mut connection = self
            .connection_pool
            .connection_tagged("commitment_generator")
            .await?;
        let pubdata_params = connection
            .blocks_dal()
            .get_l1_batch_pubdata_params(l1_batch_number)
            .await?
            .context("pubdata params are missing for L1 batch")?;
        Ok(pubdata_params.pubdata_type.into())
    }

    #[tracing::instrument(skip(self))]
    async fn step(
        &self,
        l1_batch_numbers: ops::RangeInclusive<L1BatchNumber>,
    ) -> anyhow::Result<()> {
        let iterable_numbers =
            (l1_batch_numbers.start().0..=l1_batch_numbers.end().0).map(L1BatchNumber);
        let batch_futures = iterable_numbers.map(|number| async move {
            let artifacts = self
                .process_batch(number)
                .await
                .with_context(|| format!("failed processing L1 batch #{number}"))?;
            anyhow::Ok((number, artifacts))
        });
        let artifacts = futures::future::try_join_all(batch_futures).await?;

        let mut connection = self
            .connection_pool
            .connection_tagged("commitment_generator")
            .await?;
        // Saving changes atomically is not required here; since we save batches in order, if we encounter a DB error,
        // the commitment generator will be able to recover gracefully.
        for (l1_batch_number, artifacts) in artifacts {
            let latency =
                METRICS.generate_commitment_latency_stage[&CommitmentStage::SaveResults].start();
            connection
                .blocks_dal()
                .save_l1_batch_commitment_artifacts(l1_batch_number, &artifacts)
                .await?;
            let latency = latency.observe();
            tracing::debug!(
                "Stored commitment artifacts for L1 batch #{l1_batch_number} in {latency:?}"
            );
        }

        let health_details = serde_json::json!({
            "l1_batch_number": *l1_batch_numbers.end(),
        });
        self.health_updater
            .update(Health::from(HealthStatus::Ready).with_details(health_details));
        Ok(())
    }

    fn tweak_input(&self, input: &mut CommitmentInput, commitment_mode: L1BatchCommitmentMode) {
        match (commitment_mode, input) {
            (L1BatchCommitmentMode::Rollup, _) => {
                // Do nothing
            }
            (L1BatchCommitmentMode::Validium, CommitmentInput::PostBoojum { blob_hashes, .. }) => {
                for hashes in blob_hashes {
                    hashes.commitment = H256::zero();
                }
            }
            (L1BatchCommitmentMode::Validium, _) => { /* Do nothing */ }
        }
    }

    fn post_process_commitment(
        &self,
        commitment: &mut L1BatchCommitment,
        commitment_mode: L1BatchCommitmentMode,
    ) {
        match (commitment_mode, &mut commitment.auxiliary_output) {
            (
                L1BatchCommitmentMode::Validium,
                L1BatchAuxiliaryOutput::PostBoojum { blob_hashes, .. },
            ) => {
                blob_hashes.fill(Default::default());
            }
            _ => { /* Do nothing */ }
        }
    }

    #[tracing::instrument(skip(self))]
    async fn next_batch_range(&self) -> anyhow::Result<Option<ops::RangeInclusive<L1BatchNumber>>> {
        let mut connection = self
            .connection_pool
            .connection_tagged("commitment_generator")
            .await?;
        let Some(next_batch_number) = connection
            .blocks_dal()
            .get_next_l1_batch_ready_for_commitment_generation()
            .await?
        else {
            return Ok(None);
        };

        let Some(last_batch_number) = connection
            .blocks_dal()
            .get_last_l1_batch_ready_for_commitment_generation()
            .await?
        else {
            return Ok(None);
        };
        anyhow::ensure!(
            next_batch_number <= last_batch_number,
            "Unexpected node state: next L1 batch ready for commitment generation (#{next_batch_number}) is greater than \
             the last L1 batch ready for commitment generation (#{last_batch_number})"
        );
        let last_batch_number =
            last_batch_number.min(next_batch_number + self.parallelism.get() - 1);
        Ok(Some(next_batch_number..=last_batch_number))
    }

    /// Runs this commitment generator indefinitely. It will process L1 batches added to the database
    /// processed by the Merkle tree (or a tree fetcher), with a previously configured max parallelism.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!(
            "Starting commitment generator with parallelism {}",
            self.parallelism
        );
        if self.connection_pool.max_size() < self.parallelism.get() {
            tracing::warn!(
                "Connection pool for commitment generation has fewer connections ({pool_size}) than \
                 configured max parallelism ({parallelism}); commitment generation may be slowed down as a result",
                pool_size = self.connection_pool.max_size(),
                parallelism = self.parallelism.get()
            );
        }
        self.health_updater.update(HealthStatus::Ready.into());

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, commitment generator is shutting down");
                break;
            }

            let Some(l1_batch_numbers) = self.next_batch_range().await? else {
                tokio::time::sleep(SLEEP_INTERVAL).await;
                continue;
            };

            tracing::info!("Started commitment generation for L1 batches #{l1_batch_numbers:?}");
            let step_latency = METRICS.step_latency.start();
            self.step(l1_batch_numbers.clone()).await?;
            let step_latency = step_latency.observe();
            let batch_count = l1_batch_numbers.end().0 - l1_batch_numbers.start().0 + 1;
            METRICS.step_batch_count.observe(batch_count.into());
            tracing::info!("Finished commitment generation for L1 batches #{l1_batch_numbers:?} in {step_latency:?} ({:?} per batch)", step_latency / batch_count);
        }
        Ok(())
    }
}
