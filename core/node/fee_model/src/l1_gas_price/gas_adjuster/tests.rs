use std::{collections::VecDeque, sync::RwLockReadGuard, time::Duration};

use test_casing::test_casing;
use zksync_config::GasAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::{clients::MockSettlementLayer, BaseFees};
use zksync_node_test_utils::create_l1_batch;
use zksync_types::{
    aggregated_operations::{AggregatedActionType, L1BatchAggregatedActionType},
    commitment::L1BatchCommitmentMode,
    eth_sender::EthTxFinalityStatus,
    pubdata_da::PubdataSendingMode,
    Address, L1BatchNumber, ProtocolVersion, H256,
};
use zksync_web3_decl::client::{DynClient, L1, L2};

use super::{GasAdjuster, GasStatistics, GasStatisticsInner};
use crate::l1_gas_price::GasAdjusterClient;

/// Check that we compute the median correctly
#[test]
fn median() {
    // sorted: 4 4 6 7 8
    assert_eq!(GasStatisticsInner::new(5, 5, [6, 4, 7, 8, 4]).median(), 6);
    // sorted: 4 4 8 10
    assert_eq!(GasStatisticsInner::new(4, 4, [8, 4, 4, 10]).median(), 8);
}

/// Check that we properly manage the block base fee queue
#[test]
fn samples_queue() {
    let mut stats = GasStatisticsInner::new(5, 5, [6, 4, 7, 8, 4, 5]);

    assert_eq!(stats.samples, VecDeque::from([4, 7, 8, 4, 5]));

    stats.add_samples([18, 18, 18]);

    assert_eq!(stats.samples, VecDeque::from([4, 5, 18, 18, 18]));
}

const TEST_BLOCK_FEES: [u64; 10] = [0, 4, 6, 8, 7, 5, 5, 8, 10, 9];
const TEST_BLOB_FEES: [u64; 10] = [
    0,
    393216,
    393216,
    393216 * 2,
    393216,
    393216 * 2,
    393216 * 2,
    393216 * 3,
    393216 * 4,
    393216,
];
const TEST_PUBDATA_PRICES: [u64; 10] = [
    0,
    493216,
    493216,
    493216 * 2,
    493216,
    493216 * 2,
    493216 * 2,
    493216 * 3,
    493216 * 4,
    493216,
];

fn test_config() -> GasAdjusterConfig {
    GasAdjusterConfig {
        default_priority_fee_per_gas: 5,
        max_base_fee_samples: 5,
        pricing_formula_parameter_a: 1.5,
        pricing_formula_parameter_b: 1.0005,
        internal_l1_pricing_multiplier: 0.8,
        internal_enforced_l1_gas_price: None,
        internal_enforced_pubdata_price: None,
        poll_period: Duration::from_secs(5),
        max_l1_gas_price: u64::MAX,
        num_samples_for_blob_base_fee_estimate: 3,
        internal_pubdata_pricing_multiplier: 1.0,
        max_blob_base_fee: u64::MAX,
    }
}

/// Helper function to read a value from adjuster
fn read<T>(statistics: &GasStatistics<T>) -> RwLockReadGuard<'_, GasStatisticsInner<T>> {
    statistics.0.read().unwrap()
}

/// Check that we properly fetch base fees as block are mined
#[test_casing(2, [L1BatchCommitmentMode::Rollup, L1BatchCommitmentMode::Validium])]
#[tokio::test]
async fn kept_updated(commitment_mode: L1BatchCommitmentMode) {
    let base_fees = TEST_BLOCK_FEES
        .into_iter()
        .zip(TEST_BLOB_FEES)
        .map(|(block, blob)| BaseFees {
            base_fee_per_gas: block,
            base_fee_per_blob_gas: blob.into(),
            l2_pubdata_price: 0.into(),
        })
        .collect();

    let eth_client = MockSettlementLayer::builder()
        .with_fee_history(base_fees)
        .build();
    // 5 sampled blocks + additional block to account for latest block subtraction
    eth_client.advance_block_number(6, EthTxFinalityStatus::Finalized);

    let config = test_config();
    let client: Box<DynClient<L1>> = Box::new(eth_client.clone().into_client());
    let pool = ConnectionPool::<Core>::test_pool().await;
    let adjuster = GasAdjuster::new(
        GasAdjusterClient::from(client),
        config.clone(),
        PubdataSendingMode::Calldata,
        commitment_mode,
        pool,
    )
    .await
    .unwrap();

    assert_eq!(
        read(&adjuster.base_fee_statistics).samples.len(),
        config.max_base_fee_samples
    );
    assert_eq!(read(&adjuster.base_fee_statistics).median(), 6);

    eprintln!("{:?}", read(&adjuster.blob_base_fee_statistics).samples);
    let expected_median_blob_base_fee = 393216 * 2;
    assert_eq!(
        read(&adjuster.blob_base_fee_statistics).samples.len(),
        config.num_samples_for_blob_base_fee_estimate
    );
    assert_eq!(
        read(&adjuster.blob_base_fee_statistics).median(),
        expected_median_blob_base_fee.into()
    );

    eth_client.advance_block_number(3, EthTxFinalityStatus::Finalized);
    adjuster.keep_updated().await.unwrap();

    assert_eq!(
        read(&adjuster.base_fee_statistics).samples.len(),
        config.max_base_fee_samples
    );
    assert_eq!(read(&adjuster.base_fee_statistics).median(), 7);

    let expected_median_blob_base_fee = 393216 * 3;
    assert_eq!(read(&adjuster.blob_base_fee_statistics).samples.len(), 3);
    assert_eq!(
        read(&adjuster.blob_base_fee_statistics).median(),
        expected_median_blob_base_fee.into()
    );
}

/// Check that we properly fetch base fees as block are mined
#[test_casing(2, [L1BatchCommitmentMode::Rollup, L1BatchCommitmentMode::Validium])]
#[tokio::test]
async fn kept_updated_l2(commitment_mode: L1BatchCommitmentMode) {
    let base_fees = TEST_BLOCK_FEES
        .into_iter()
        .zip(TEST_PUBDATA_PRICES)
        .map(|(block, pubdata)| BaseFees {
            base_fee_per_gas: block,
            base_fee_per_blob_gas: 1.into(),
            l2_pubdata_price: pubdata.into(),
        })
        .collect();

    let eth_client = MockSettlementLayer::<L2>::builder()
        .with_fee_history(base_fees)
        .build();
    // 5 sampled blocks + additional block to account for latest block subtraction
    eth_client.advance_block_number(6, EthTxFinalityStatus::Finalized);

    let config = test_config();
    let client: Box<DynClient<L2>> = Box::new(eth_client.clone().into_client());
    let pool = ConnectionPool::<Core>::test_pool().await;

    let adjuster = GasAdjuster::new(
        GasAdjusterClient::from(client),
        config.clone(),
        PubdataSendingMode::RelayedL2Calldata,
        commitment_mode,
        pool,
    )
    .await
    .unwrap();

    assert_eq!(
        read(&adjuster.base_fee_statistics).samples.len(),
        config.max_base_fee_samples
    );
    assert_eq!(read(&adjuster.base_fee_statistics).median(), 6);

    eprintln!("{:?}", read(&adjuster.l2_pubdata_price_statistics).samples);
    let expected_median_blob_base_fee = 493216 * 2;
    assert_eq!(
        read(&adjuster.l2_pubdata_price_statistics).samples.len(),
        config.num_samples_for_blob_base_fee_estimate
    );
    assert_eq!(
        read(&adjuster.l2_pubdata_price_statistics).median(),
        expected_median_blob_base_fee.into()
    );

    eth_client.advance_block_number(3, EthTxFinalityStatus::Finalized);
    adjuster.keep_updated().await.unwrap();

    assert_eq!(
        read(&adjuster.base_fee_statistics).samples.len(),
        config.max_base_fee_samples
    );
    assert_eq!(read(&adjuster.base_fee_statistics).median(), 7);

    let expected_median_blob_base_fee = 493216 * 3;
    assert_eq!(read(&adjuster.l2_pubdata_price_statistics).samples.len(), 3);
    assert_eq!(
        read(&adjuster.l2_pubdata_price_statistics).median(),
        expected_median_blob_base_fee.into()
    );
}

async fn blob_gas_adjuster(pool: ConnectionPool<Core>) -> GasAdjuster {
    let base_fees = TEST_BLOCK_FEES
        .into_iter()
        .zip(TEST_BLOB_FEES)
        .map(|(block, blob)| BaseFees {
            base_fee_per_gas: block,
            base_fee_per_blob_gas: blob.into(),
            l2_pubdata_price: 0.into(),
        })
        .collect();
    let eth_client = MockSettlementLayer::builder()
        .with_fee_history(base_fees)
        .build();
    eth_client.advance_block_number(6, EthTxFinalityStatus::Finalized);

    let client: Box<DynClient<L1>> = Box::new(eth_client.into_client());
    GasAdjuster::new(
        GasAdjusterClient::from(client),
        test_config(),
        PubdataSendingMode::Blobs,
        L1BatchCommitmentMode::Rollup,
        pool,
    )
    .await
    .unwrap()
}

fn median_blob_pubdata_price(adjuster: &GasAdjuster) -> u64 {
    let median = read(&adjuster.blob_base_fee_statistics).median().as_u64();
    (median as f64 * adjuster.config.internal_pubdata_pricing_multiplier) as u64
}

/// Inserts L1 batch #1, committed by a finalized tx sent at `commit_sent_at_block`, and sealed
/// L1 batches #2 and #3 with `blobs_per_batch` blobs each that are not committed yet.
async fn insert_pending_blobs(
    pool: &ConnectionPool<Core>,
    commit_sent_at_block: u32,
    blobs_per_batch: usize,
) {
    let mut conn = pool.connection().await.unwrap();
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    for number in 1..=3 {
        let mut header = create_l1_batch(number);
        // `insert_mock_l1_batch()` uses 1 byte per blob.
        header.pubdata_input = Some(vec![0; blobs_per_batch]);
        conn.blocks_dal()
            .insert_mock_l1_batch(&header)
            .await
            .unwrap();
    }

    let commit_action = AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Commit);
    let eth_tx = conn
        .eth_sender_dal()
        .save_eth_tx(
            0,
            vec![],
            commit_action,
            Address::zero(),
            None,
            None,
            None,
            false,
        )
        .await
        .unwrap();
    conn.blocks_dal()
        .set_eth_tx_id_for_l1_batches(
            L1BatchNumber(1)..=L1BatchNumber(1),
            eth_tx.id,
            commit_action,
        )
        .await
        .unwrap();
    let tx_hash = H256::repeat_byte(1);
    conn.eth_sender_dal()
        .insert_tx_history(
            eth_tx.id,
            0,
            0,
            None,
            None,
            tx_hash,
            &[],
            commit_sent_at_block,
            None,
        )
        .await
        .unwrap();
    conn.eth_sender_dal()
        .confirm_tx(tx_hash, EthTxFinalityStatus::Finalized, 0.into())
        .await
        .unwrap();
}

/// The pubdata price estimate runs on every fee-input request. In the API this includes every
/// `eth_call`, `eth_estimateGas` and `eth_sendRawTransaction` made while no L1 batch is open.
/// So it must not query L1: a per-request L1 call scales L1 traffic with API traffic
/// (INC-84). The mock client panics on `eth_getBlockByNumber(latest)`, which is what the blob
/// base fee predictor used to send.
#[tokio::test]
async fn blob_pubdata_price_estimate_does_not_query_l1() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let adjuster = blob_gas_adjuster(pool).await;
    adjuster.keep_updated().await.unwrap();

    // No batches are pending, so the prediction equals the median blob base fee.
    let expected = median_blob_pubdata_price(&adjuster);
    for _ in 0..3 {
        assert_eq!(adjuster.estimate_effective_pubdata_price(), expected);
    }
}

/// The blob base fee prediction is computed on each `keep_updated()` poll, not per request.
#[tokio::test]
async fn blob_pubdata_price_estimate_uses_prediction_from_last_poll() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let adjuster = blob_gas_adjuster(pool.clone()).await;
    let median_price = median_blob_pubdata_price(&adjuster);
    assert_eq!(adjuster.estimate_effective_pubdata_price(), median_price);

    let l1_head = adjuster.base_fee_statistics.last_processed_block() as u32;
    insert_pending_blobs(&pool, l1_head - 1, 10).await;
    // The new pending blobs are only picked up on the next poll.
    assert_eq!(adjuster.estimate_effective_pubdata_price(), median_price);

    adjuster.keep_updated().await.unwrap();
    let predicted_price = adjuster.estimate_effective_pubdata_price();
    assert!(
        predicted_price > median_price,
        "{predicted_price} <= {median_price}"
    );
    assert_eq!(adjuster.estimate_effective_pubdata_price(), predicted_price);
}

/// If the gas adjuster's L1 head is behind the block at which the last commit tx was sent,
/// the prediction falls back to the median instead of jumping to `max_blob_base_fee`.
#[tokio::test]
async fn blob_fee_prediction_falls_back_to_median_if_l1_head_is_behind_commit() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let adjuster = blob_gas_adjuster(pool.clone()).await;
    let l1_head = adjuster.base_fee_statistics.last_processed_block() as u32;
    insert_pending_blobs(&pool, l1_head + 100, 10).await;

    adjuster.keep_updated().await.unwrap();
    assert_eq!(
        adjuster.estimate_effective_pubdata_price(),
        median_blob_pubdata_price(&adjuster)
    );
}
