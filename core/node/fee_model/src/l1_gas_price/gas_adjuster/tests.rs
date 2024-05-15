use std::collections::VecDeque;

use test_casing::test_casing;
use zksync_config::{configs::eth_sender::PubdataSendingMode, GasAdjusterConfig};
use zksync_eth_client::clients::MockEthereum;
use zksync_types::commitment::L1BatchCommitmentMode;

use super::{GasAdjuster, GasStatisticsInner};

/// Check that we compute the median correctly
#[test]
fn median() {
    // sorted: 4 4 6 7 8
    assert_eq!(GasStatisticsInner::new(5, 5, &[6, 4, 7, 8, 4]).median(), 6);
    // sorted: 4 4 8 10
    assert_eq!(GasStatisticsInner::new(4, 4, &[8, 4, 4, 10]).median(), 8);
}

/// Check that we properly manage the block base fee queue
#[test]
fn samples_queue() {
    let mut stats = GasStatisticsInner::new(5, 5, &[6, 4, 7, 8, 4, 5]);

    assert_eq!(stats.samples, VecDeque::from([4, 7, 8, 4, 5]));

    stats.add_samples(&[18, 18, 18]);

    assert_eq!(stats.samples, VecDeque::from([4, 5, 18, 18, 18]));
}

/// Check that we properly fetch base fees as block are mined
#[test_casing(2, [L1BatchCommitmentMode::Rollup, L1BatchCommitmentMode::Validium])]
#[tokio::test]
async fn kept_updated(commitment_mode: L1BatchCommitmentMode) {
    let eth_client = Box::new(
        MockEthereum::default()
            .with_fee_history(vec![0, 4, 6, 8, 7, 5, 5, 8, 10, 9])
            .with_excess_blob_gas_history(vec![
                393216,
                393216 * 2,
                393216,
                393216 * 2,
                393216,
                393216 * 2,
                393216 * 3,
                393216 * 4,
            ]),
    );
    eth_client.advance_block_number(5);

    let adjuster = GasAdjuster::new(
        eth_client.clone(),
        GasAdjusterConfig {
            default_priority_fee_per_gas: 5,
            max_base_fee_samples: 5,
            pricing_formula_parameter_a: 1.5,
            pricing_formula_parameter_b: 1.0005,
            internal_l1_pricing_multiplier: 0.8,
            internal_enforced_l1_gas_price: None,
            internal_enforced_pubdata_price: None,
            poll_period: 5,
            max_l1_gas_price: None,
            num_samples_for_blob_base_fee_estimate: 3,
            internal_pubdata_pricing_multiplier: 1.0,
            max_blob_base_fee: None,
        },
        PubdataSendingMode::Calldata,
        commitment_mode,
    )
    .await
    .unwrap();

    assert_eq!(
        adjuster.base_fee_statistics.0.read().unwrap().samples.len(),
        5
    );
    assert_eq!(adjuster.base_fee_statistics.0.read().unwrap().median(), 6);

    let expected_median_blob_base_fee = GasAdjuster::blob_base_fee(393216);
    assert_eq!(
        adjuster
            .blob_base_fee_statistics
            .0
            .read()
            .unwrap()
            .samples
            .len(),
        1
    );
    assert_eq!(
        adjuster.blob_base_fee_statistics.0.read().unwrap().median(),
        expected_median_blob_base_fee
    );

    eth_client.advance_block_number(3);
    adjuster.keep_updated().await.unwrap();

    assert_eq!(
        adjuster.base_fee_statistics.0.read().unwrap().samples.len(),
        5
    );
    assert_eq!(adjuster.base_fee_statistics.0.read().unwrap().median(), 7);

    let expected_median_blob_base_fee = GasAdjuster::blob_base_fee(393216 * 3);
    assert_eq!(
        adjuster
            .blob_base_fee_statistics
            .0
            .read()
            .unwrap()
            .samples
            .len(),
        3
    );
    assert_eq!(
        adjuster.blob_base_fee_statistics.0.read().unwrap().median(),
        expected_median_blob_base_fee
    );
}

#[test]
fn blob_base_fee_formula() {
    const EXCESS_BLOB_GAS: u64 = 0x4b80000;
    const EXPECTED_BLOB_BASE_FEE: u64 = 19893400088;

    let blob_base_fee = GasAdjuster::blob_base_fee(EXCESS_BLOB_GAS);
    assert_eq!(blob_base_fee.as_u64(), EXPECTED_BLOB_BASE_FEE);
}
