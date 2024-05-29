use std::collections::HashMap;

use assert_matches::assert_matches;
use test_log::test;
use zksync_dal::pruning_dal::PruningInfo;
use zksync_db_connection::connection::Connection;
use zksync_health_check::CheckHealth;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{
    create_l1_batch, create_l1_batch_metadata, create_l2_block,
    l1_batch_metadata_to_commitment_artifacts,
};
use zksync_types::{
    aggregated_operations::AggregatedActionType, block::L2BlockHeader, Address, L2BlockNumber,
    ProtocolVersion, H256,
};

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
        self.is_batch_prunable_responses
            .get(&l1_batch_number)
            .cloned()
            .context("error!")
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
            removal_delay: Duration::ZERO,
            pruned_batch_chunk_size: 1,
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

async fn insert_l2_blocks(
    conn: &mut Connection<'_, Core>,
    l1_batches_count: u64,
    l2_blocks_per_batch: u64,
) {
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    for l1_batch_number in 0..l1_batches_count {
        for l2_block_index in 0..l2_blocks_per_batch {
            let l2_block_number =
                L2BlockNumber((l1_batch_number * l2_blocks_per_batch + l2_block_index) as u32);
            let l2_block_header = L2BlockHeader {
                number: l2_block_number,
                timestamp: 0,
                hash: H256::from_low_u64_be(u64::from(l2_block_number.0)),
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
                .insert_l2_block(&l2_block_header)
                .await
                .unwrap();
            conn.blocks_dal()
                .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(l1_batch_number as u32))
                .await
                .unwrap();
        }
    }
}

#[test(tokio::test)]
async fn hard_pruning_ignores_conditions_checks() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();

    insert_l2_blocks(&mut conn, 10, 2).await;
    conn.pruning_dal()
        .soft_prune_batches_range(L1BatchNumber(2), L2BlockNumber(5))
        .await
        .unwrap();

    let nothing_prunable_check = Arc::new(ConditionMock::name("nothing prunable"));
    let pruner = DbPruner::with_conditions(
        DbPrunerConfig {
            removal_delay: Duration::ZERO,
            pruned_batch_chunk_size: 5,
            minimum_l1_batch_age: Duration::ZERO,
        },
        pool.clone(),
        vec![nothing_prunable_check],
    );
    let health_check = pruner.health_check();

    pruner.run_single_iteration().await.unwrap();

    assert_eq!(
        PruningInfo {
            last_soft_pruned_l1_batch: Some(L1BatchNumber(2)),
            last_soft_pruned_l2_block: Some(L2BlockNumber(5)),
            last_hard_pruned_l1_batch: Some(L1BatchNumber(2)),
            last_hard_pruned_l2_block: Some(L2BlockNumber(5)),
        },
        conn.pruning_dal().get_pruning_info().await.unwrap()
    );
    let health = health_check.check_health().await;
    assert_matches!(health.status(), HealthStatus::Ready);
}
#[test(tokio::test)]
async fn pruner_catches_up_with_hard_pruning_up_to_soft_pruning_boundary_ignoring_chunk_size() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    insert_l2_blocks(&mut conn, 10, 2).await;
    conn.pruning_dal()
        .soft_prune_batches_range(L1BatchNumber(2), L2BlockNumber(5))
        .await
        .unwrap();

    let pruner = DbPruner::with_conditions(
        DbPrunerConfig {
            removal_delay: Duration::ZERO,
            pruned_batch_chunk_size: 5,
            minimum_l1_batch_age: Duration::ZERO,
        },
        pool.clone(),
        vec![], //No checks, so every batch is prunable
    );

    pruner.run_single_iteration().await.unwrap();

    assert_eq!(
        PruningInfo {
            last_soft_pruned_l1_batch: Some(L1BatchNumber(2)),
            last_soft_pruned_l2_block: Some(L2BlockNumber(5)),
            last_hard_pruned_l1_batch: Some(L1BatchNumber(2)),
            last_hard_pruned_l2_block: Some(L2BlockNumber(5)),
        },
        conn.pruning_dal().get_pruning_info().await.unwrap()
    );

    pruner.run_single_iteration().await.unwrap();
    assert_eq!(
        PruningInfo {
            last_soft_pruned_l1_batch: Some(L1BatchNumber(7)),
            last_soft_pruned_l2_block: Some(L2BlockNumber(15)),
            last_hard_pruned_l1_batch: Some(L1BatchNumber(7)),
            last_hard_pruned_l2_block: Some(L2BlockNumber(15)),
        },
        conn.pruning_dal().get_pruning_info().await.unwrap()
    );
}

#[test(tokio::test)]
async fn unconstrained_pruner_with_fresh_database() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();

    insert_l2_blocks(&mut conn, 10, 2).await;

    let pruner = DbPruner::with_conditions(
        DbPrunerConfig {
            removal_delay: Duration::ZERO,
            pruned_batch_chunk_size: 3,
            minimum_l1_batch_age: Duration::ZERO,
        },
        pool.clone(),
        vec![], //No checks, so every batch is prunable
    );

    pruner.run_single_iteration().await.unwrap();

    assert_eq!(
        PruningInfo {
            last_soft_pruned_l1_batch: Some(L1BatchNumber(3)),
            last_soft_pruned_l2_block: Some(L2BlockNumber(7)),
            last_hard_pruned_l1_batch: Some(L1BatchNumber(3)),
            last_hard_pruned_l2_block: Some(L2BlockNumber(7)),
        },
        conn.pruning_dal().get_pruning_info().await.unwrap()
    );

    pruner.run_single_iteration().await.unwrap();
    assert_eq!(
        PruningInfo {
            last_soft_pruned_l1_batch: Some(L1BatchNumber(6)),
            last_soft_pruned_l2_block: Some(L2BlockNumber(13)),
            last_hard_pruned_l1_batch: Some(L1BatchNumber(6)),
            last_hard_pruned_l2_block: Some(L2BlockNumber(13)),
        },
        conn.pruning_dal().get_pruning_info().await.unwrap()
    );
}

#[test(tokio::test)]
async fn pruning_blocked_after_first_chunk() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    insert_l2_blocks(&mut conn, 10, 2).await;

    let first_chunk_prunable_check =
        Arc::new(ConditionMock::name("first chunk prunable").with_response(L1BatchNumber(3), true));

    let pruner = DbPruner::with_conditions(
        DbPrunerConfig {
            removal_delay: Duration::ZERO,
            pruned_batch_chunk_size: 3,
            minimum_l1_batch_age: Duration::ZERO,
        },
        pool.clone(),
        vec![first_chunk_prunable_check],
    );
    pruner.run_single_iteration().await.unwrap();

    assert_eq!(
        PruningInfo {
            last_soft_pruned_l1_batch: Some(L1BatchNumber(3)),
            last_soft_pruned_l2_block: Some(L2BlockNumber(7)),
            last_hard_pruned_l1_batch: Some(L1BatchNumber(3)),
            last_hard_pruned_l2_block: Some(L2BlockNumber(7)),
        },
        conn.pruning_dal().get_pruning_info().await.unwrap()
    );

    pruner.run_single_iteration().await.unwrap();
    // pruning shouldn't have progressed as chunk 6 cannot be pruned
    assert_eq!(
        PruningInfo {
            last_soft_pruned_l1_batch: Some(L1BatchNumber(3)),
            last_soft_pruned_l2_block: Some(L2BlockNumber(7)),
            last_hard_pruned_l1_batch: Some(L1BatchNumber(3)),
            last_hard_pruned_l2_block: Some(L2BlockNumber(7)),
        },
        conn.pruning_dal().get_pruning_info().await.unwrap()
    );
}

#[tokio::test]
async fn pruner_is_resistant_to_errors() {
    let pool = ConnectionPool::<Core>::test_pool().await;

    // This condition returns `true` despite the batch not present in Postgres.
    let erroneous_condition =
        Arc::new(ConditionMock::name("always returns true").with_response(L1BatchNumber(3), true));

    let pruner = DbPruner::with_conditions(
        DbPrunerConfig {
            removal_delay: Duration::ZERO,
            pruned_batch_chunk_size: 3,
            minimum_l1_batch_age: Duration::ZERO,
        },
        pool.clone(),
        vec![erroneous_condition],
    );
    pruner.run_single_iteration().await.unwrap_err();

    let mut health_check = pruner.health_check();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let pruner_task_handle = tokio::spawn(pruner.run(stop_receiver));

    let health = health_check
        .wait_for(|health| matches!(health.status(), HealthStatus::Affected))
        .await;
    let health_details = health.details().unwrap();
    let error = health_details["error"].as_str().unwrap();
    // Matching error messages is an anti-pattern, but we essentially test UX here.
    assert!(
        error.contains("L1 batch #3 is ready to be pruned, but has no L2 blocks"),
        "{error}"
    );

    stop_sender.send_replace(true);
    pruner_task_handle.await.unwrap().unwrap();
}

/// Seals an L1 batch with a single L2 block.
async fn seal_l1_batch(storage: &mut Connection<'_, Core>, number: u32) {
    let block_header = create_l2_block(number);
    storage
        .blocks_dal()
        .insert_l2_block(&block_header)
        .await
        .unwrap();

    let header = create_l1_batch(number);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&header)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(number))
        .await
        .unwrap();
}

async fn save_l1_batch_metadata(storage: &mut Connection<'_, Core>, number: u32) {
    let metadata = create_l1_batch_metadata(number);
    storage
        .blocks_dal()
        .save_l1_batch_tree_data(L1BatchNumber(number), &metadata.tree_data())
        .await
        .unwrap();
    storage
        .blocks_dal()
        .save_l1_batch_commitment_artifacts(
            L1BatchNumber(number),
            &l1_batch_metadata_to_commitment_artifacts(&metadata),
        )
        .await
        .unwrap();
}

async fn mark_l1_batch_as_executed(storage: &mut Connection<'_, Core>, number: u32) {
    storage
        .eth_sender_dal()
        .insert_bogus_confirmed_eth_tx(
            L1BatchNumber(number),
            AggregatedActionType::Execute,
            H256::from_low_u64_be(number.into()),
            chrono::Utc::now(),
        )
        .await
        .unwrap();
}

async fn mark_l1_batch_as_consistent(storage: &mut Connection<'_, Core>, number: u32) {
    storage
        .blocks_dal()
        .set_consistency_checker_last_processed_l1_batch(L1BatchNumber(number))
        .await
        .unwrap();
}

async fn collect_conditions_output(
    conditions: &[Arc<dyn PruneCondition>],
    number: L1BatchNumber,
) -> Vec<bool> {
    let mut output = Vec::with_capacity(conditions.len());
    for condition in conditions {
        output.push(condition.is_batch_prunable(number).await.unwrap());
    }
    output
}

#[tokio::test]
async fn real_conditions_work_as_expected() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let conditions: Vec<Arc<dyn PruneCondition>> = vec![
        Arc::new(L1BatchExistsCondition { pool: pool.clone() }),
        Arc::new(NextL1BatchHasMetadataCondition { pool: pool.clone() }),
        Arc::new(NextL1BatchWasExecutedCondition { pool: pool.clone() }),
        Arc::new(ConsistencyCheckerProcessedBatch { pool: pool.clone() }),
    ];

    assert_eq!(
        collect_conditions_output(&conditions, L1BatchNumber(1)).await,
        [false; 4]
    );

    // Add 2 batches to the storage.
    for number in 1..=2 {
        seal_l1_batch(&mut storage, number).await;
    }
    assert_eq!(
        collect_conditions_output(&conditions, L1BatchNumber(1)).await,
        [true, false, false, false]
    );

    // Add metadata for both batches.
    for number in 1..=2 {
        save_l1_batch_metadata(&mut storage, number).await;
    }
    assert_eq!(
        collect_conditions_output(&conditions, L1BatchNumber(1)).await,
        [true, true, false, false]
    );

    // Mark both batches as executed.
    for number in 1..=2 {
        mark_l1_batch_as_executed(&mut storage, number).await;
    }
    assert_eq!(
        collect_conditions_output(&conditions, L1BatchNumber(1)).await,
        [true, true, true, false]
    );

    // Mark both batches as consistent.
    for number in 1..=2 {
        mark_l1_batch_as_consistent(&mut storage, number).await;
    }
    assert_eq!(
        collect_conditions_output(&conditions, L1BatchNumber(1)).await,
        [true, true, true, true]
    );
}

#[tokio::test]
async fn pruner_with_real_conditions() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let config = DbPrunerConfig {
        removal_delay: Duration::from_millis(10), // non-zero to not have a tight loop in `DbPruner::run()`
        pruned_batch_chunk_size: 1,
        minimum_l1_batch_age: Duration::ZERO,
    };
    let pruner = DbPruner::new(config, pool.clone());
    let mut health_check = pruner.health_check();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let pruner_handle = tokio::spawn(pruner.run(stop_receiver));

    let batch_handles = (1_u32..=5).map(|number| {
        let pool = pool.clone();
        tokio::spawn(async move {
            // Emulate producing batches with overlapping life cycle.
            tokio::time::sleep(Duration::from_millis(u64::from(number) * 10)).await;

            let mut storage = pool.connection().await.unwrap();
            seal_l1_batch(&mut storage, number).await;
            tokio::time::sleep(Duration::from_millis(15)).await;
            save_l1_batch_metadata(&mut storage, number).await;
            tokio::time::sleep(Duration::from_millis(12)).await;
            mark_l1_batch_as_consistent(&mut storage, number).await;
            tokio::time::sleep(Duration::from_millis(17)).await;
            mark_l1_batch_as_executed(&mut storage, number).await;
        })
    });

    // Wait until all batches went through their life cycle.
    for handle in batch_handles {
        handle.await.unwrap();
    }

    health_check
        .wait_for(|health| {
            if !matches!(health.status(), HealthStatus::Ready) {
                return false;
            }
            let Some(details) = health.details() else {
                return false;
            };
            let details: DbPrunerHealth = serde_json::from_value(details.clone()).unwrap();
            details.last_hard_pruned_l1_batch == Some(L1BatchNumber(4))
        })
        .await;

    stop_sender.send_replace(true);
    pruner_handle.await.unwrap().unwrap();
}
