use super::{GasAdjuster, GasStatisticsInner};
use std::collections::VecDeque;
use std::sync::Arc;
use zksync_config::GasAdjusterConfig;
use zksync_eth_client::clients::mock::MockEthereum;

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
#[tokio::test]
async fn kept_updated() {
    let eth_client =
        Arc::new(MockEthereum::default().with_fee_history(vec![0, 4, 6, 8, 7, 5, 5, 8, 10, 9]));
    eth_client.advance_block_number(5);

    let adjuster = GasAdjuster::new(
        Arc::clone(&eth_client),
        GasAdjusterConfig {
            default_priority_fee_per_gas: 5,
            max_base_fee_samples: 5,
            pricing_formula_parameter_a: 1.5,
            pricing_formula_parameter_b: 1.0005,
            internal_l1_pricing_multiplier: 0.8,
            internal_enforced_l1_gas_price: None,
            poll_period: 5,
            max_l1_gas_price: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(adjuster.statistics.0.read().unwrap().samples.len(), 5);
    assert_eq!(adjuster.statistics.0.read().unwrap().median(), 6);

    eth_client.advance_block_number(3);
    adjuster.keep_updated().await.unwrap();

    assert_eq!(adjuster.statistics.0.read().unwrap().samples.len(), 5);
    assert_eq!(adjuster.statistics.0.read().unwrap().median(), 7);
}
