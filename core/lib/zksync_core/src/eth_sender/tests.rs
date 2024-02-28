use std::sync::Arc;

use zksync_dal::ConnectionPool;
use zksync_types::l1_batch_commit_data_generator::{
    RollupModeL1BatchCommitDataGenerator, ValidiumModeL1BatchCommitDataGenerator,
};

use super::tests_helpers::{self, EthSenderTester};

// Tests that we send multiple transactions and confirm them all in one iteration.
#[tokio::test]
async fn confirm_many() -> anyhow::Result<()> {
    let mut rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![10; 100],
        false,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;

    let mut validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![10; 100],
        false,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;

    tests_helpers::confirm_many(&mut rollup_tester).await?;
    tests_helpers::confirm_many(&mut validium_tester).await
}

// Tests that we resend first un-mined transaction every block with an increased gas price.
#[tokio::test]
async fn resend_each_block() -> anyhow::Result<()> {
    let mut rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![7, 6, 5, 5, 5, 2, 1],
        false,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    let mut validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![7, 6, 5, 5, 5, 2, 1],
        false,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;

    tests_helpers::resend_each_block(&mut rollup_tester).await?;
    tests_helpers::resend_each_block(&mut validium_tester).await
}

// Tests that if transaction was mined, but not enough blocks has been mined since,
// we won't mark it as confirmed but also won't resend it.
#[tokio::test]
async fn dont_resend_already_mined() -> anyhow::Result<()> {
    let mut rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    let mut validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;

    tests_helpers::dont_resend_already_mined(&mut rollup_tester).await?;
    tests_helpers::dont_resend_already_mined(&mut validium_tester).await
}

#[tokio::test]
async fn three_scenarios() -> anyhow::Result<()> {
    let mut rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    let mut validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;

    tests_helpers::three_scenarios(&mut rollup_tester).await?;
    tests_helpers::three_scenarios(&mut validium_tester).await
}

#[should_panic(expected = "We can't operate after tx fail")]
#[tokio::test]
async fn failed_eth_tx() {
    let mut rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    let mut validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;

    tests_helpers::failed_eth_tx(&mut rollup_tester).await;
    tests_helpers::failed_eth_tx(&mut validium_tester).await
}

#[tokio::test]
async fn correct_order_for_confirmations() -> anyhow::Result<()> {
    let mut rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    let mut validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;

    tests_helpers::correct_order_for_confirmations(
        &mut rollup_tester,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await?;
    tests_helpers::correct_order_for_confirmations(
        &mut validium_tester,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await
}

#[tokio::test]
async fn skipped_l1_batch_at_the_start() -> anyhow::Result<()> {
    let mut rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        true,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}).clone(),
    )
    .await;

    let mut validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        true,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}).clone(),
    )
    .await;

    tests_helpers::skipped_l1_batch_at_the_start(
        &mut rollup_tester,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await?;
    tests_helpers::skipped_l1_batch_at_the_start(
        &mut validium_tester,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await
}

#[tokio::test]
async fn skipped_l1_batch_in_the_middle() -> anyhow::Result<()> {
    let mut rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        true,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}).clone(),
    )
    .await;

    let mut validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        true,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}).clone(),
    )
    .await;

    tests_helpers::skipped_l1_batch_in_the_middle(
        &mut rollup_tester,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await?;
    tests_helpers::skipped_l1_batch_in_the_middle(
        &mut validium_tester,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await
}

#[tokio::test]
async fn test_parse_multicall_data() {
    let rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    let validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;

    tests_helpers::test_parse_multicall_data(&rollup_tester).await;
    tests_helpers::test_parse_multicall_data(&validium_tester).await
}

#[tokio::test]
async fn get_multicall_data() {
    let mut rollup_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(RollupModeL1BatchCommitDataGenerator {}),
    )
    .await;
    let mut validium_tester = EthSenderTester::new(
        ConnectionPool::test_pool().await,
        vec![100; 100],
        false,
        Arc::new(ValidiumModeL1BatchCommitDataGenerator {}),
    )
    .await;

    tests_helpers::get_multicall_data(&mut rollup_tester).await;
    tests_helpers::get_multicall_data(&mut validium_tester).await
}
