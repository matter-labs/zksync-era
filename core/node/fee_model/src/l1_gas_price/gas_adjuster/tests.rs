use std::{collections::VecDeque, sync::RwLockReadGuard};

use test_casing::test_casing;
use zksync_config::{configs::eth_sender::PubdataSendingMode, GasAdjusterConfig};
use zksync_eth_client::{clients::MockEthereum, BaseFees};
use zksync_types::commitment::L1BatchCommitmentMode;

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

/// Check that we properly fetch base fees as block are mined
#[test_casing(2, [L1BatchCommitmentMode::Rollup, L1BatchCommitmentMode::Validium])]
#[tokio::test]
async fn kept_updated(commitment_mode: L1BatchCommitmentMode) {
    // Helper function to read a value from adjuster
    fn read<T>(statistics: &GasStatistics<T>) -> RwLockReadGuard<GasStatisticsInner<T>> {
        statistics.0.read().unwrap()
    }

    let block_fees = vec![0, 4, 6, 8, 7, 5, 5, 8, 10, 9];
    let blob_fees = vec![
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
    let base_fees = block_fees
        .into_iter()
        .zip(blob_fees)
        .map(|(block, blob)| BaseFees {
            base_fee_per_gas: block,
            base_fee_per_blob_gas: blob.into(),
        })
        .collect();

    let eth_client = MockEthereum::builder().with_fee_history(base_fees).build();
    // 5 sampled blocks + additional block to account for latest block subtraction
    eth_client.advance_block_number(6);

    let config = GasAdjusterConfig {
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
        l2_mode: None,
    };
    let adjuster = GasAdjuster::new(
        GasAdjusterClient::from_l1(Box::new(eth_client.clone().into_client())),
        config,
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

    eth_client.advance_block_number(3);
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
