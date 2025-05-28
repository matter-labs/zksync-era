use std::{collections::VecDeque, sync::RwLockReadGuard, time::Duration};

use test_casing::test_casing;
use zksync_config::GasAdjusterConfig;
use zksync_eth_client::{clients::MockSettlementLayer, BaseFees};
use zksync_types::{
    commitment::L1BatchCommitmentMode, eth_sender::EthTxFinalityStatus,
    pubdata_da::PubdataSendingMode,
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
fn read<T>(statistics: &GasStatistics<T>) -> RwLockReadGuard<GasStatisticsInner<T>> {
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
    let adjuster = GasAdjuster::new(
        GasAdjusterClient::from(client),
        config.clone(),
        PubdataSendingMode::Calldata,
        commitment_mode,
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

    let adjuster = GasAdjuster::new(
        GasAdjusterClient::from(client),
        config.clone(),
        PubdataSendingMode::RelayedL2Calldata,
        commitment_mode,
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
