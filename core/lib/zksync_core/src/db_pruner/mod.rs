mod metrics;
pub mod prune_conditions;

use std::{fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::L1BatchNumber;

use crate::db_pruner::metrics::{MetricPruneType, METRICS};

#[derive(Debug)]
pub struct DbPrunerConfig {
    pub soft_and_hard_pruning_time_delta: Duration,
    pub next_iterations_delay: Duration,
    pub pruned_chunk_size: u32,
}

impl fmt::Debug for dyn PruneCondition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PruneCondition")
            .field("name", &self.name())
            .finish()
    }
}

#[derive(Debug)]
pub struct DbPruner {
    config: DbPrunerConfig,
    prune_conditions: Vec<Arc<dyn PruneCondition>>,
}

/// Interface to be used for health checks.
#[async_trait]
pub trait PruneCondition: Send + Sync + 'static {
    /// Unique name of the condition.
    fn name(&self) -> &'static str;
    async fn is_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> anyhow::Result<bool>;
}

impl DbPruner {
    pub fn new(
        config: DbPrunerConfig,
        prune_conditions: Vec<Arc<dyn PruneCondition>>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            prune_conditions,
        })
    }

    pub async fn is_l1_batch_prunable(&self, l1_batch_number: L1BatchNumber) -> bool {
        let mut successful_conditions: Vec<&'static str> = vec![];
        let mut failed_conditions: Vec<&'static str> = vec![];
        let mut errored_conditions: Vec<&'static str> = vec![];

        for condition in self.prune_conditions.iter() {
            match condition.is_batch_prunable(l1_batch_number).await {
                Ok(true) => successful_conditions.push(condition.name()),
                Ok(false) => failed_conditions.push(condition.name()),
                Err(error) => {
                    errored_conditions.push(condition.name());
                    tracing::warn!(
                        "Pruning condition for component {} resulted in an error: {error}",
                        condition.name()
                    )
                }
            }
        }
        let result = failed_conditions.is_empty() && errored_conditions.is_empty();
        if !result {
            tracing::info!(
                "Pruning l1 batch {l1_batch_number} is not possible, \
            successful checks: {successful_conditions:?}, \
            failed conditions: {failed_conditions:?}, \
            errored_conditions: {errored_conditions:?}"
            );
        }
        result
    }

    async fn update_l1_batches_metric(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let mut storage = pool.connection().await.unwrap();
        let first_l1_batch = storage.blocks_dal().get_earliest_l1_batch_number().await?;
        let last_l1_batch = storage.blocks_dal().get_sealed_l1_batch_number().await?;
        if first_l1_batch.is_none() {
            METRICS.not_pruned_l1_batches_count.set(0);
            return Ok(());
        }

        METRICS
            .not_pruned_l1_batches_count
            .set((last_l1_batch.unwrap().0 - first_l1_batch.unwrap().0) as u64);
        Ok(())
    }

    async fn soft_prune(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<bool> {
        let latency = METRICS.pruning_chunk_duration[&MetricPruneType::Soft].start();

        let mut storage = pool.connection().await?;
        let mut transaction = storage.start_transaction().await?;

        let current_pruning_info = transaction.pruning_dal().get_pruning_info().await?;
        let next_l1_batch_to_prune = L1BatchNumber(
            current_pruning_info
                .last_soft_pruned_l1_batch
                .unwrap_or(L1BatchNumber(0))
                .0
                + self.config.pruned_chunk_size,
        );
        if !self.is_l1_batch_prunable(next_l1_batch_to_prune).await {
            latency.observe();
            return Ok(false);
        }

        let next_miniblock_to_prune = transaction
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(next_l1_batch_to_prune)
            .await?
            .unwrap()
            .1;
        transaction
            .pruning_dal()
            .soft_prune_batches_range(next_l1_batch_to_prune, next_miniblock_to_prune)
            .await?;

        transaction.commit().await?;

        let latency = latency.observe();
        tracing::info!(
            "Soft pruned db l1_batches up to {} and miniblocks up to {}, operation took {:?}",
            next_l1_batch_to_prune,
            next_miniblock_to_prune,
            latency
        );

        Ok(true)
    }

    async fn hard_prune(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let latency = METRICS.pruning_chunk_duration[&MetricPruneType::Hard].start();

        let mut storage = pool.connection().await?;
        let mut transaction = storage.start_transaction().await?;

        let current_pruning_info = transaction.pruning_dal().get_pruning_info().await?;
        transaction
            .pruning_dal()
            .hard_prune_batches_range(
                current_pruning_info.last_soft_pruned_l1_batch.unwrap(),
                current_pruning_info.last_soft_pruned_miniblock.unwrap(),
            )
            .await?;

        transaction.commit().await?;

        let latency = latency.observe();
        tracing::info!(
            "Hard pruned db l1_batches up to {} and miniblocks up to {}, operation took {:?}",
            current_pruning_info.last_soft_pruned_l1_batch.unwrap(),
            current_pruning_info.last_soft_pruned_miniblock.unwrap(),
            latency
        );

        Ok(())
    }

    pub async fn run_single_iteration(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<bool> {
        let mut storage = pool.connection().await?;
        let current_pruning_info = storage.pruning_dal().get_pruning_info().await?;

        if current_pruning_info.last_soft_pruned_l1_batch
            == current_pruning_info.last_hard_pruned_l1_batch
        {
            let pruning_done = self.soft_prune(pool).await?;
            if !pruning_done {
                return Ok(false);
            }

            tokio::time::sleep(self.config.soft_and_hard_pruning_time_delta).await;
        }

        self.hard_prune(pool).await?;

        Ok(true)
    }
    pub async fn run_in_loop(
        self,
        pool: ConnectionPool<Core>,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, shutting down DbPruner");
            }
            let _ = self.update_l1_batches_metric(&pool).await;
            // as this component is not really mission-critical, all errors are generally ignored
            let pruning_done = self.run_single_iteration(&pool).await;
            if !pruning_done.unwrap_or(false) {
                tokio::time::sleep(self.config.next_iterations_delay).await;
            }
        }
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

    #[async_trait]
    impl PruneCondition for ConditionMock {
        fn name(&self) -> &'static str {
            self.name
        }

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
        let pruner = DbPruner::new(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::from_secs(0),
                pruned_chunk_size: 1,
                next_iterations_delay: Duration::from_secs(0),
            },
            vec![failing_check, other_failing_check],
        )
        .unwrap();
        // first check succeeds, but second returns an error
        assert!(!pruner.is_l1_batch_prunable(L1BatchNumber(1)).await);
        //second check fails
        assert!(!pruner.is_l1_batch_prunable(L1BatchNumber(2)).await);
        //first check fails
        assert!(!pruner.is_l1_batch_prunable(L1BatchNumber(3)).await);

        assert!(pruner.is_l1_batch_prunable(L1BatchNumber(4)).await);
    }

    async fn insert_miniblocks(
        conn: &mut Connection<'_, Core>,
        l1_batches_count: u64,
        miniblocks_per_batch: u64,
    ) {
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

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
        let pruner = DbPruner::new(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::from_secs(0),
                pruned_chunk_size: 5,
                next_iterations_delay: Duration::from_secs(0),
            },
            vec![nothing_prunable_check],
        )
        .unwrap();

        pruner.run_single_iteration(&pool).await.unwrap();

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
        let pruner = DbPruner::new(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::from_secs(0),
                pruned_chunk_size: 5,
                next_iterations_delay: Duration::from_secs(0),
            },
            vec![], //No checks, so every batch is prunable
        )
        .unwrap();

        pruner.run_single_iteration(&pool).await.unwrap();

        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(2)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(5)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(2)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(5)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );

        pruner.run_single_iteration(&pool).await.unwrap();
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

        let pruner = DbPruner::new(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::from_secs(0),
                pruned_chunk_size: 3,
                next_iterations_delay: Duration::from_secs(0),
            },
            vec![], //No checks, so every batch is prunable
        )
        .unwrap();

        pruner.run_single_iteration(&pool).await.unwrap();

        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(7)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(7)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );

        pruner.run_single_iteration(&pool).await.unwrap();
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

        let pruner = DbPruner::new(
            DbPrunerConfig {
                soft_and_hard_pruning_time_delta: Duration::from_secs(0),
                pruned_chunk_size: 3,
                next_iterations_delay: Duration::from_secs(0),
            },
            vec![first_chunk_prunable_check],
        )
        .unwrap();

        pruner.run_single_iteration(&pool).await.unwrap();

        assert_eq!(
            PruningInfo {
                last_soft_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_soft_pruned_miniblock: Some(MiniblockNumber(7)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(3)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(7)),
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );

        pruner.run_single_iteration(&pool).await.unwrap();
        //pruning shouldn't have progressed as chunk 6 cannot be pruned
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
