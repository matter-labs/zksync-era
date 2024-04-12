//! Postgres pruning component.

use std::{fmt, sync::Arc, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{pruning_dal::HardPruningStats, Connection, ConnectionPool, Core, CoreDal};
use zksync_types::L1BatchNumber;

use self::{
    metrics::{MetricPruneType, METRICS},
    prune_conditions::{
        ConsistencyCheckerProcessedBatch, L1BatchExistsCondition, L1BatchOlderThanPruneCondition,
        NextL1BatchHasMetadataCondition, NextL1BatchWasExecutedCondition,
    },
};

mod metrics;
mod prune_conditions;

/// Configuration
#[derive(Debug)]
pub struct DbPrunerConfig {
    /// Delta between soft- and hard-removing data from Postgres.
    pub soft_and_hard_pruning_time_delta: Duration,
    /// Sleep interval between pruning iterations.
    pub next_iterations_delay: Duration,
    /// Number of L1 batches pruned at a time. The pruner will do nothing if there is less than this number
    /// of batches to prune.
    pub pruned_batch_chunk_size: u32,
    /// Minimum age of an L1 batch in order for it to be eligible for pruning. Setting this to zero
    /// will effectively disable this pruning criterion.
    pub minimum_l1_batch_age: Duration,
}

/// Postgres database pruning component.
#[derive(Debug)]
pub struct DbPruner {
    config: DbPrunerConfig,
    connection_pool: ConnectionPool<Core>,
    prune_conditions: Vec<Arc<dyn PruneCondition>>,
}

/// Interface to be used for health checks.
#[async_trait]
trait PruneCondition: fmt::Debug + fmt::Display + Send + Sync + 'static {
    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool>;
}

impl DbPruner {
    pub fn new(config: DbPrunerConfig, connection_pool: ConnectionPool<Core>) -> Self {
        let conditions: Vec<Arc<dyn PruneCondition>> = vec![
            Arc::new(L1BatchExistsCondition {
                conn: connection_pool.clone(),
            }),
            Arc::new(NextL1BatchHasMetadataCondition {
                conn: connection_pool.clone(),
            }),
            Arc::new(NextL1BatchWasExecutedCondition {
                conn: connection_pool.clone(),
            }),
            Arc::new(L1BatchOlderThanPruneCondition {
                minimum_age: config.minimum_l1_batch_age,
                conn: connection_pool.clone(),
            }),
            Arc::new(ConsistencyCheckerProcessedBatch {
                conn: connection_pool.clone(),
            }),
        ];
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
            prune_conditions,
        }
    }

    async fn is_l1_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> bool {
        let mut successful_conditions = vec![];
        let mut failed_conditions = vec![];
        let mut errored_conditions = vec![];

        for condition in &self.prune_conditions {
            match condition.is_batch_prunable(l1_batch_number).await {
                Ok(true) => successful_conditions.push(condition.to_string()),
                Ok(false) => failed_conditions.push(condition.to_string()),
                Err(error) => {
                    errored_conditions.push(condition.to_string());
                    tracing::warn!("Pruning condition '{condition}' resulted in an error: {error}");
                }
            }
        }
        let result = failed_conditions.is_empty() && errored_conditions.is_empty();
        if !result {
            tracing::info!(
                "Pruning l1 batch {l1_batch_number} is not possible, \
                successful conditions: {successful_conditions:?}, \
                failed conditions: {failed_conditions:?}, \
                errored_conditions: {errored_conditions:?}"
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

    async fn soft_prune(&self, storage: &mut Connection<'_, Core>) -> anyhow::Result<bool> {
        let latency = METRICS.pruning_chunk_duration[&MetricPruneType::Soft].start();
        let mut transaction = storage.start_transaction().await?;

        let current_pruning_info = transaction.pruning_dal().get_pruning_info().await?;
        let next_l1_batch_to_prune = L1BatchNumber(
            current_pruning_info
                .last_soft_pruned_l1_batch
                .unwrap_or(L1BatchNumber(0))
                .0
                + self.config.pruned_batch_chunk_size,
        );
        if !self.is_l1_batch_prunable(next_l1_batch_to_prune).await {
            latency.observe();
            return Ok(false);
        }

        let (_, next_miniblock_to_prune) = transaction
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(next_l1_batch_to_prune)
            .await?
            .with_context(|| format!("L1 batch #{next_l1_batch_to_prune} is ready to be pruned, but has no miniblocks"))?;
        transaction
            .pruning_dal()
            .soft_prune_batches_range(next_l1_batch_to_prune, next_miniblock_to_prune)
            .await?;

        transaction.commit().await?;

        let latency = latency.observe();
        tracing::info!(
            "Soft pruned db l1_batches up to {next_l1_batch_to_prune} and miniblocks up to {next_miniblock_to_prune}, operation took {latency:?}",
        );

        Ok(true)
    }

    async fn hard_prune(&self, storage: &mut Connection<'_, Core>) -> anyhow::Result<()> {
        let latency = METRICS.pruning_chunk_duration[&MetricPruneType::Hard].start();
        let mut transaction = storage.start_transaction().await?;

        let current_pruning_info = transaction.pruning_dal().get_pruning_info().await?;
        let last_soft_pruned_l1_batch =
            current_pruning_info.last_soft_pruned_l1_batch.with_context(|| {
                format!("bogus pruning info {current_pruning_info:?}: trying to hard-prune data, but there is no soft-pruned L1 batch")
            })?;
        let last_soft_pruned_miniblock =
            current_pruning_info.last_soft_pruned_miniblock.with_context(|| {
                format!("bogus pruning info {current_pruning_info:?}: trying to hard-prune data, but there is no soft-pruned miniblock")
            })?;

        let stats = transaction
            .pruning_dal()
            .hard_prune_batches_range(last_soft_pruned_l1_batch, last_soft_pruned_miniblock)
            .await?;
        Self::report_hard_pruning_stats(stats);
        transaction.commit().await?;

        let mut storage = self.connection_pool.connection_tagged("db_pruner").await?;
        storage
            .pruning_dal()
            .run_vacuum_after_hard_pruning()
            .await?;

        let latency = latency.observe();
        tracing::info!(
            "Hard pruned db l1_batches up to {last_soft_pruned_l1_batch} and miniblocks up to {last_soft_pruned_miniblock}, \
            operation took {latency:?}"
        );
        Ok(())
    }

    fn report_hard_pruning_stats(stats: HardPruningStats) {
        let HardPruningStats {
            deleted_l1_batches,
            deleted_miniblocks,
            deleted_storage_logs_from_past_batches,
            deleted_storage_logs_from_pruned_batches,
            deleted_events,
            deleted_call_traces,
            deleted_l2_to_l1_logs,
        } = stats;
        let deleted_storage_logs =
            deleted_storage_logs_from_past_batches + deleted_storage_logs_from_pruned_batches;
        tracing::info!(
            "Performed pruning of database, deleted {deleted_l1_batches} L1 batches, {deleted_miniblocks} miniblocks, \
             {deleted_storage_logs} storage logs ({deleted_storage_logs_from_pruned_batches} from pruned batches + \
             {deleted_storage_logs_from_past_batches} from past batches), \
             {deleted_events} events, {deleted_call_traces} call traces, {deleted_l2_to_l1_logs} L2-to-L1 logs"
        );
    }

    async fn run_single_iteration(&self) -> anyhow::Result<bool> {
        let mut storage = self.connection_pool.connection_tagged("db_pruner").await?;
        let current_pruning_info = storage.pruning_dal().get_pruning_info().await?;

        // If this `if` is not entered, it means that the node has restarted after soft pruning
        if current_pruning_info.last_soft_pruned_l1_batch
            == current_pruning_info.last_hard_pruned_l1_batch
        {
            let pruning_done = self.soft_prune(&mut storage).await?;
            if !pruning_done {
                return Ok(false);
            }
        }
        drop(storage); // Don't hold a connection across a timeout

        tokio::time::sleep(self.config.soft_and_hard_pruning_time_delta).await;
        let mut storage = self.connection_pool.connection_tagged("db_pruner").await?;
        self.hard_prune(&mut storage).await?;
        Ok(true)
    }
    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        while !*stop_receiver.borrow_and_update() {
            if let Err(err) = self.update_l1_batches_metric().await {
                tracing::warn!("Error updating DB pruning metrics: {err:?}");
            }

            let should_sleep = match self.run_single_iteration().await {
                Err(err) => {
                    // As this component is not really mission-critical, all errors are generally ignored
                    tracing::warn!(
                        "Pruning error, retrying in {:?}, error was: {err:?}",
                        self.config.next_iterations_delay
                    );
                    true
                }
                Ok(pruning_done) => !pruning_done,
            };

            if should_sleep
                && tokio::time::timeout(self.config.next_iterations_delay, stop_receiver.changed())
                    .await
                    .is_ok()
            {
                // The pruner either received a stop signal, or the stop receiver was dropped. In any case,
                // the pruner should exit.
                break;
            }
        }
        tracing::info!("Stop signal received, shutting down DB pruning");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use anyhow::anyhow;
    use multivm::zk_evm_latest::ethereum_types::H256;
    use test_log::test;
    use zksync_dal::pruning_dal::PruningInfo;
    use zksync_db_connection::connection::Connection;
    use zksync_types::{block::MiniblockHeader, Address, MiniblockNumber, ProtocolVersion};

    use super::*;

    #[derive(Debug)]
    struct ConditionMock {
        pub name: &'static str,
        pub is_batch_prunable_responses: HashMap<L1BatchNumber, bool>,
    }

    impl ConditionMock {
        fn name(name: &'static str) -> ConditionMock {
            Self {
                name,
                is_batch_prunable_responses: HashMap::default(),
            }
        }

        fn with_response(mut self, l1_batch_number: L1BatchNumber, value: bool) -> Self {
            self.is_batch_prunable_responses
                .insert(l1_batch_number, value);
            self
        }
    }

    impl fmt::Display for ConditionMock {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "{}", self.name)
        }
    }

    #[async_trait]
    impl PruneCondition for ConditionMock {
        async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool> {
            if !self
                .is_batch_prunable_responses
                .contains_key(&l1_batch_number)
            {
                return Err(anyhow!("Error!"));
            }
            Ok(self
                .is_batch_prunable_responses
                .get(&l1_batch_number)
                .cloned()
                .unwrap())
        }
    }

    #[test(tokio::test)]
    async fn is_l1_batch_prunable_works() {
        let failing_check = Arc::new(
            ConditionMock::name("some failing some passing1")
                .with_response(L1BatchNumber(1), true)
                .with_response(L1BatchNumber(2), true)
                .with_response(L1BatchNumber(3), false)
                .with_response(L1BatchNumber(4), true),
        );
        let other_failing_check = Arc::new(
            ConditionMock::name("some failing some passing2")
                .with_response(L1BatchNumber(2), false)
                .with_response(L1BatchNumber(3), true)
                .with_response(L1BatchNumber(4), true),
        );
        let pruner = DbPruner::with_conditions(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::ZERO,
                pruned_batch_chunk_size: 1,
                next_iterations_delay: Duration::ZERO,
                minimum_l1_batch_age: Duration::ZERO,
            },
            ConnectionPool::test_pool().await,
            vec![failing_check, other_failing_check],
        );
        // first check succeeds, but second returns an error
        assert!(!pruner.is_l1_batch_prunable(L1BatchNumber(1)).await);
        // second check fails
        assert!(!pruner.is_l1_batch_prunable(L1BatchNumber(2)).await);
        // first check fails
        assert!(!pruner.is_l1_batch_prunable(L1BatchNumber(3)).await);

        assert!(pruner.is_l1_batch_prunable(L1BatchNumber(4)).await);
    }

    async fn insert_miniblocks(
        conn: &mut Connection<'_, Core>,
        l1_batches_count: u64,
        miniblocks_per_batch: u64,
    ) {
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        for l1_batch_number in 0..l1_batches_count {
            for miniblock_index in 0..miniblocks_per_batch {
                let miniblock_number = MiniblockNumber(
                    (l1_batch_number * miniblocks_per_batch + miniblock_index) as u32,
                );
                let miniblock_header = MiniblockHeader {
                    number: miniblock_number,
                    timestamp: 0,
                    hash: H256::from_low_u64_be(u64::from(miniblock_number.0)),
                    l1_tx_count: 0,
                    l2_tx_count: 0,
                    fee_account_address: Address::repeat_byte(1),
                    base_fee_per_gas: 0,
                    gas_per_pubdata_limit: 0,
                    batch_fee_input: Default::default(),
                    base_system_contracts_hashes: Default::default(),
                    protocol_version: Some(Default::default()),
                    virtual_blocks: 0,
                    gas_limit: 0,
                };

                conn.blocks_dal()
                    .insert_miniblock(&miniblock_header)
                    .await
                    .unwrap();
                conn.blocks_dal()
                    .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(l1_batch_number as u32))
                    .await
                    .unwrap();
            }
        }
    }

    #[test(tokio::test)]
    async fn hard_pruning_ignores_conditions_checks() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        insert_miniblocks(&mut conn, 10, 2).await;
        conn.pruning_dal()
            .soft_prune_batches_range(L1BatchNumber(2), MiniblockNumber(5))
            .await
            .unwrap();

        let nothing_prunable_check = Arc::new(ConditionMock::name("nothing prunable"));
        let pruner = DbPruner::with_conditions(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::ZERO,
                pruned_batch_chunk_size: 5,
                next_iterations_delay: Duration::ZERO,
                minimum_l1_batch_age: Duration::ZERO,
            },
            pool.clone(),
            vec![nothing_prunable_check],
        );

        pruner.run_single_iteration().await.unwrap();

        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(2)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(5)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(2)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(5)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );
    }
    #[test(tokio::test)]
    async fn pruner_should_catch_up_with_hard_pruning_up_to_soft_pruning_boundary_ignoring_chunk_size(
    ) {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        insert_miniblocks(&mut conn, 10, 2).await;
        conn.pruning_dal()
            .soft_prune_batches_range(L1BatchNumber(2), MiniblockNumber(5))
            .await
            .unwrap();
        let pruner = DbPruner::with_conditions(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::ZERO,
                pruned_batch_chunk_size: 5,
                next_iterations_delay: Duration::ZERO,
                minimum_l1_batch_age: Duration::ZERO,
            },
            pool.clone(),
            vec![], //No checks, so every batch is prunable
        );

        pruner.run_single_iteration().await.unwrap();

        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(2)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(5)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(2)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(5)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );

        pruner.run_single_iteration().await.unwrap();
        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(7)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(15)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(7)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(15)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );
    }

    #[test(tokio::test)]
    async fn unconstrained_pruner_with_fresh_database() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        insert_miniblocks(&mut conn, 10, 2).await;

        let pruner = DbPruner::with_conditions(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::ZERO,
                pruned_batch_chunk_size: 3,
                next_iterations_delay: Duration::ZERO,
                minimum_l1_batch_age: Duration::ZERO,
            },
            pool.clone(),
            vec![], //No checks, so every batch is prunable
        );

        pruner.run_single_iteration().await.unwrap();

        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(7)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(7)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );

        pruner.run_single_iteration().await.unwrap();
        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(6)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(13)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(6)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(13)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );
    }

    #[test(tokio::test)]
    async fn pruning_blocked_after_first_chunk() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        insert_miniblocks(&mut conn, 10, 2).await;

        let first_chunk_prunable_check = Arc::new(
            ConditionMock::name("first chunk prunable").with_response(L1BatchNumber(3), true),
        );

        let pruner = DbPruner::with_conditions(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::ZERO,
                pruned_batch_chunk_size: 3,
                next_iterations_delay: Duration::ZERO,
                minimum_l1_batch_age: Duration::ZERO,
            },
            pool.clone(),
            vec![first_chunk_prunable_check],
        );

        pruner.run_single_iteration().await.unwrap();

        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(7)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(7)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );

        pruner.run_single_iteration().await.unwrap();
        // pruning shouldn't have progressed as chunk 6 cannot be pruned
        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(7)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(7)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );
    }
}
