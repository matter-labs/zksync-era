use std::collections::HashMap;

use anyhow::anyhow;
use multivm::zk_evm_latest::ethereum_types::H256;
use test_log::test;
use zksync_dal::pruning_dal::PruningInfo;
use zksync_db_connection::connection::Connection;
use zksync_types::{block::L2BlockHeader, Address, L2BlockNumber, ProtocolVersion};

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
            let miniblock_number =
                L2BlockNumber((l1_batch_number * miniblocks_per_batch + miniblock_index) as u32);
            let miniblock_header = L2BlockHeader {
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
                .insert_l2_block(&miniblock_header)
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

    insert_miniblocks(&mut conn, 10, 2).await;
    conn.pruning_dal()
        .soft_prune_batches_range(L1BatchNumber(2), L2BlockNumber(5))
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
            last_soft_pruned_l2_block: Some(L2BlockNumber(5)),
            last_hard_pruned_l1_batch: Some(L1BatchNumber(2)),
            last_hard_pruned_l2_block: Some(L2BlockNumber(5)),
        },
        conn.pruning_dal().get_pruning_info().await.unwrap()
    );
}
#[test(tokio::test)]
async fn pruner_should_catch_up_with_hard_pruning_up_to_soft_pruning_boundary_ignoring_chunk_size()
{
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();

    insert_miniblocks(&mut conn, 10, 2).await;
    conn.pruning_dal()
        .soft_prune_batches_range(L1BatchNumber(2), L2BlockNumber(5))
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

    insert_miniblocks(&mut conn, 10, 2).await;

    let first_chunk_prunable_check =
        Arc::new(ConditionMock::name("first chunk prunable").with_response(L1BatchNumber(3), true));

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
