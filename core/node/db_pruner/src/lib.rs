//! Postgres pruning component.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use zksync_dal::{
    pruning_dal::{HardPruningInfo, PruningInfo, SoftPruningInfo},
    Connection, ConnectionPool, Core, CoreDal,
};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{L1BatchNumber, L2BlockNumber, OrStopped};

use self::{
    metrics::{ConditionOutcome, PruneType, METRICS},
    prune_conditions::{
        ConsistencyCheckerProcessedBatch, L1BatchExistsCondition, L1BatchOlderThanPruneCondition,
        NextL1BatchHasMetadataCondition, NextL1BatchWasExecutedCondition, PruneCondition,
    },
};

mod metrics;
mod prune_conditions;
#[cfg(test)]
mod tests;

/// Configuration
#[derive(Debug)]
pub struct DbPrunerConfig {
    /// Delta between soft- and hard-removing data from Postgres.
    pub removal_delay: Duration,
    /// Number of L1 batches pruned at a time. The pruner will do nothing if there is less than this number
    /// of batches to prune.
    pub pruned_batch_chunk_size: u32,
    /// Minimum age of an L1 batch in order for it to be eligible for pruning. Setting this to zero
    /// will effectively disable this pruning criterion.
    pub minimum_l1_batch_age: Duration,
}

#[derive(Debug, Serialize, Deserialize)]
struct DbPrunerHealth {
    #[serde(skip_serializing_if = "Option::is_none")]
    last_soft_pruned_l1_batch: Option<L1BatchNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_soft_pruned_l2_block: Option<L2BlockNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_hard_pruned_l1_batch: Option<L1BatchNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_hard_pruned_l2_block: Option<L2BlockNumber>,
}

impl From<PruningInfo> for DbPrunerHealth {
    fn from(info: PruningInfo) -> Self {
        Self {
            last_soft_pruned_l1_batch: info.last_soft_pruned.map(|info| info.l1_batch),
            last_soft_pruned_l2_block: info.last_soft_pruned.map(|info| info.l2_block),
            last_hard_pruned_l1_batch: info.last_hard_pruned.map(|info| info.l1_batch),
            last_hard_pruned_l2_block: info.last_hard_pruned.map(|info| info.l2_block),
        }
    }
}

/// Outcome of a single pruning iteration.
#[derive(Debug)]
enum PruningIterationOutcome {
    /// Nothing to prune.
    NoOp,
    /// Iteration resulted in pruning.
    Pruned,
}

/// Postgres database pruning component.
#[derive(Debug)]
pub struct DbPruner {
    config: DbPrunerConfig,
    connection_pool: ConnectionPool<Core>,
    health_updater: HealthUpdater,
    prune_conditions: Vec<Arc<dyn PruneCondition>>,
}

impl DbPruner {
    pub fn new(config: DbPrunerConfig, connection_pool: ConnectionPool<Core>) -> Self {
        let mut conditions: Vec<Arc<dyn PruneCondition>> = vec![
            Arc::new(L1BatchExistsCondition {
                pool: connection_pool.clone(),
            }),
            Arc::new(NextL1BatchHasMetadataCondition {
                pool: connection_pool.clone(),
            }),
            Arc::new(NextL1BatchWasExecutedCondition {
                pool: connection_pool.clone(),
            }),
            Arc::new(ConsistencyCheckerProcessedBatch {
                pool: connection_pool.clone(),
            }),
        ];
        if config.minimum_l1_batch_age > Duration::ZERO {
            // Do not add a condition if it's trivial in order to not clutter logs.
            conditions.push(Arc::new(L1BatchOlderThanPruneCondition {
                minimum_age: config.minimum_l1_batch_age,
                pool: connection_pool.clone(),
            }));
        }

        Self::with_conditions(config, connection_pool, conditions)
    }

    fn with_conditions(
        config: DbPrunerConfig,
        connection_pool: ConnectionPool<Core>,
        prune_conditions: Vec<Arc<dyn PruneCondition>>,
    ) -> Self {
        Self {
            config,
            connection_pool,
            health_updater: ReactiveHealthCheck::new("db_pruner").1,
            prune_conditions,
        }
    }

    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    async fn is_l1_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> bool {
        let mut successful_conditions = vec![];
        let mut failed_conditions = vec![];
        let mut errored_conditions = vec![];

        for condition in &self.prune_conditions {
            let outcome = match condition.is_batch_prunable(l1_batch_number).await {
                Ok(true) => {
                    successful_conditions.push(condition.to_string());
                    ConditionOutcome::Success
                }
                Ok(false) => {
                    failed_conditions.push(condition.to_string());
                    ConditionOutcome::Fail
                }
                Err(error) => {
                    errored_conditions.push(condition.to_string());
                    tracing::warn!("Pruning condition '{condition}' resulted in an error: {error}");
                    ConditionOutcome::Error
                }
            };
            METRICS.observe_condition(condition.as_ref(), outcome);
        }

        let result = failed_conditions.is_empty() && errored_conditions.is_empty();
        if !result {
            tracing::debug!(
                "Pruning L1 batch {l1_batch_number} is not possible, \
                 successful conditions: {successful_conditions:?}, \
                 failed conditions: {failed_conditions:?}, \
                 errored conditions: {errored_conditions:?}"
            );
        }
        result
    }

    async fn update_l1_batches_metric(&self) -> anyhow::Result<()> {
        let mut storage = self.connection_pool.connection_tagged("db_pruner").await?;
        let first_l1_batch = storage.blocks_dal().get_earliest_l1_batch_number().await?;
        let last_l1_batch = storage.blocks_dal().get_sealed_l1_batch_number().await?;
        let Some(first_l1_batch) = first_l1_batch else {
            METRICS.not_pruned_l1_batches_count.set(0);
            return Ok(());
        };

        let last_l1_batch = last_l1_batch
            .context("unreachable DB state: there's an earliest L1 batch, but no latest one")?;
        METRICS
            .not_pruned_l1_batches_count
            .set((last_l1_batch.0 - first_l1_batch.0).into());
        Ok(())
    }

    fn update_health(&self, info: PruningInfo) {
        let health = Health::from(HealthStatus::Ready).with_details(DbPrunerHealth::from(info));
        self.health_updater.update(health);
    }

    async fn soft_prune(&self, storage: &mut Connection<'_, Core>) -> anyhow::Result<bool> {
        let start = Instant::now();
        let mut transaction = storage.start_transaction().await?;

        let mut current_pruning_info = transaction.pruning_dal().get_pruning_info().await?;
        let next_l1_batch_to_prune = current_pruning_info
            .last_soft_pruned
            .map_or(L1BatchNumber(0), |info| info.l1_batch)
            + self.config.pruned_batch_chunk_size;
        if !self.is_l1_batch_prunable(next_l1_batch_to_prune).await {
            METRICS.pruning_chunk_duration[&PruneType::NoOp].observe(start.elapsed());
            return Ok(false);
        }

        let (_, next_l2_block_to_prune) = transaction
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(next_l1_batch_to_prune)
            .await?
            .with_context(|| format!("L1 batch #{next_l1_batch_to_prune} is ready to be pruned, but has no L2 blocks"))?;
        transaction
            .pruning_dal()
            .insert_soft_pruning_log(next_l1_batch_to_prune, next_l2_block_to_prune)
            .await?;

        transaction.commit().await?;

        let latency = start.elapsed();
        METRICS.pruning_chunk_duration[&PruneType::Soft].observe(latency);
        tracing::info!(
            "Soft pruned db l1_batches up to {next_l1_batch_to_prune} and L2 blocks up to {next_l2_block_to_prune}, operation took {latency:?}",
        );

        current_pruning_info.last_soft_pruned = Some(SoftPruningInfo {
            l1_batch: next_l1_batch_to_prune,
            l2_block: next_l2_block_to_prune,
        });
        self.update_health(current_pruning_info);
        Ok(true)
    }

    async fn hard_prune(
        &self,
        storage: &mut Connection<'_, Core>,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> Result<PruningIterationOutcome, OrStopped> {
        let latency = METRICS.pruning_chunk_duration[&PruneType::Hard].start();
        let mut transaction = storage.start_transaction().await?;

        let mut current_pruning_info = transaction.pruning_dal().get_pruning_info().await?;
        let soft_pruned = current_pruning_info.last_soft_pruned.with_context(|| {
            format!("bogus pruning info {current_pruning_info:?}: trying to hard-prune data, but there is no soft-pruned data")
        })?;

        let last_pruned_l1_batch_root_hash = transaction
            .blocks_dal()
            .get_l1_batch_state_root(soft_pruned.l1_batch)
            .await?
            .with_context(|| {
                format!(
                    "hard-pruned L1 batch #{} does not have root hash",
                    soft_pruned.l1_batch
                )
            })?;

        let mut dal = transaction.pruning_dal();
        let stats = tokio::select! {
            result = dal.hard_prune_batches_range(
                soft_pruned.l1_batch,
                soft_pruned.l2_block,
            ) => result?,

            _ = stop_receiver.changed() => {
                // `hard_prune_batches_range()` can take a long time. It looks better to roll back it explicitly here if a node is getting shut down
                // rather than waiting a node to force-exit after a timeout, which would interrupt the DB connection and will lead to an implicit rollback.
                tracing::info!("Hard pruning interrupted; rolling back pruning transaction");
                transaction.rollback().await?;
                return Err(OrStopped::Stopped);
            }
        };
        METRICS.observe_hard_pruning(stats);

        dal.insert_hard_pruning_log(
            soft_pruned.l1_batch,
            soft_pruned.l2_block,
            last_pruned_l1_batch_root_hash,
        )
        .await?;
        transaction.commit().await?;

        let latency = latency.observe();
        let hard_pruning_info = HardPruningInfo {
            l1_batch: soft_pruned.l1_batch,
            l2_block: soft_pruned.l2_block,
            l1_batch_root_hash: Some(last_pruned_l1_batch_root_hash),
        };
        tracing::info!("Hard pruned data up to {hard_pruning_info:?}, operation took {latency:?}");
        current_pruning_info.last_hard_pruned = Some(hard_pruning_info);
        self.update_health(current_pruning_info);
        Ok(PruningIterationOutcome::Pruned)
    }

    async fn run_single_iteration(
        &self,
        stop_receiver: &mut watch::Receiver<bool>,
    ) -> Result<PruningIterationOutcome, OrStopped> {
        let mut storage = self.connection_pool.connection_tagged("db_pruner").await?;
        let current_pruning_info = storage.pruning_dal().get_pruning_info().await?;
        self.update_health(current_pruning_info);

        // If this `if` is not entered, it means that the node has restarted after soft pruning
        if current_pruning_info.is_caught_up() {
            let pruning_done = self.soft_prune(&mut storage).await?;
            if !pruning_done {
                return Ok(PruningIterationOutcome::NoOp);
            }
        }
        drop(storage); // Don't hold a connection across a timeout

        if tokio::time::timeout(self.config.removal_delay, stop_receiver.changed())
            .await
            .is_ok()
        {
            return Err(OrStopped::Stopped);
        }

        let mut storage = self.connection_pool.connection_tagged("db_pruner").await?;
        self.hard_prune(&mut storage, stop_receiver).await
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let next_iteration_delay = self.config.removal_delay / 2;
        tracing::info!(
            "Starting Postgres pruning with configuration {:?}, prune conditions {:?}",
            self.config,
            self.prune_conditions
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );

        while !*stop_receiver.borrow_and_update() {
            if let Err(err) = self.update_l1_batches_metric().await {
                tracing::warn!("Error updating DB pruning metrics: {err:?}");
            }

            let should_sleep = match self.run_single_iteration(&mut stop_receiver).await {
                Err(OrStopped::Internal(err)) => {
                    // As this component is not really mission-critical, all errors are generally ignored
                    tracing::warn!(
                        "Pruning error, retrying in {next_iteration_delay:?}, error was: {err:?}"
                    );
                    let health =
                        Health::from(HealthStatus::Affected).with_details(serde_json::json!({
                            "error": err.to_string(),
                        }));
                    self.health_updater.update(health);
                    true
                }
                Err(OrStopped::Stopped) => break,
                Ok(PruningIterationOutcome::Pruned) => false,
                Ok(PruningIterationOutcome::NoOp) => true,
            };

            if should_sleep
                && tokio::time::timeout(next_iteration_delay, stop_receiver.changed())
                    .await
                    .is_ok()
            {
                // The pruner either received a stop request, or the stop receiver was dropped. In any case,
                // the pruner should exit.
                break;
            }
        }
        tracing::info!("Stop request received, shutting down DB pruning");
        Ok(())
    }
}
