use std::{fmt, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::fee_model::{
    BaseTokenConversionRatio, BatchFeeInput, FeeModelConfig, FeeParams, FeeParamsV1, FeeParamsV2,
};

use crate::l1_gas_price::GasAdjuster;

pub mod l1_gas_price;
pub mod node;

/// Trait responsible for providing numerator and denominator for adjusting gas price that is denominated
/// in a non-eth base token
#[async_trait]
pub trait BaseTokenRatioProvider: fmt::Debug + Send + Sync + 'static {
    fn get_conversion_ratio(&self) -> BaseTokenConversionRatio;
}

#[async_trait]
impl BaseTokenRatioProvider for BaseTokenConversionRatio {
    fn get_conversion_ratio(&self) -> BaseTokenConversionRatio {
        *self
    }
}

/// Trait responsible for providing fee info for a batch
#[async_trait]
pub trait BatchFeeModelInputProvider: fmt::Debug + 'static + Send + Sync {
    /// Returns the batch fee with scaling applied. This may be used to account for the fact that the L1 gas and pubdata prices may fluctuate, esp.
    /// in API methods that should return values that are valid for some period of time after the estimation was done.
    async fn get_batch_fee_input_scaled(
        &self,
        l1_gas_price_scale_factor: f64,
        l1_pubdata_price_scale_factor: f64,
    ) -> anyhow::Result<BatchFeeInput> {
        let params = self.get_fee_model_params();
        Ok(params.scale(l1_gas_price_scale_factor, l1_pubdata_price_scale_factor))
    }

    /// Returns the fee model parameters using the denomination of the base token used (WEI for ETH).
    fn get_fee_model_params(&self) -> FeeParams;
}

impl dyn BatchFeeModelInputProvider {
    /// Returns the batch fee input as-is, i.e. without any scaling for the L1 gas and pubdata prices.
    pub async fn get_batch_fee_input(&self) -> anyhow::Result<BatchFeeInput> {
        self.get_batch_fee_input_scaled(1.0, 1.0).await
    }
}

/// The struct that represents the batch fee input provider to be used in the main node of the server.
///
/// This struct gets the L1 gas price directly from the provider rather than from another node, as is the
/// case with the external node.
#[derive(Debug)]
pub struct MainNodeFeeInputProvider {
    provider: Arc<GasAdjuster>,
    base_token_ratio_provider: Arc<dyn BaseTokenRatioProvider>,
    config: FeeModelConfig,
}

#[async_trait]
impl BatchFeeModelInputProvider for MainNodeFeeInputProvider {
    fn get_fee_model_params(&self) -> FeeParams {
        match self.config {
            FeeModelConfig::V1(config) => FeeParams::V1(FeeParamsV1 {
                config,
                l1_gas_price: self.provider.estimate_effective_gas_price(),
            }),
            FeeModelConfig::V2(config) => FeeParams::V2(FeeParamsV2::new(
                config,
                self.provider.estimate_effective_gas_price(),
                self.provider.estimate_effective_pubdata_price(),
                self.base_token_ratio_provider.get_conversion_ratio(),
            )),
        }
    }
}

impl MainNodeFeeInputProvider {
    pub fn new(
        provider: Arc<GasAdjuster>,
        base_token_ratio_provider: Arc<dyn BaseTokenRatioProvider>,
        config: FeeModelConfig,
    ) -> Self {
        Self {
            provider,
            base_token_ratio_provider,
            config,
        }
    }
}

/// The fee model provider to be used in the API. It returns the maximum batch fee input between the projected main node one and
/// the one from the last sealed L2 block.
#[derive(Debug)]
pub struct ApiFeeInputProvider {
    inner: Arc<dyn BatchFeeModelInputProvider>,
    connection_pool: ConnectionPool<Core>,
}

impl ApiFeeInputProvider {
    pub fn new(
        inner: Arc<dyn BatchFeeModelInputProvider>,
        connection_pool: ConnectionPool<Core>,
    ) -> Self {
        Self {
            inner,
            connection_pool,
        }
    }
}

#[async_trait]
impl BatchFeeModelInputProvider for ApiFeeInputProvider {
    async fn get_batch_fee_input_scaled(
        &self,
        l1_gas_price_scale_factor: f64,
        l1_pubdata_price_scale_factor: f64,
    ) -> anyhow::Result<BatchFeeInput> {
        Ok(self
            .inner
            .get_batch_fee_input_scaled(l1_gas_price_scale_factor, l1_pubdata_price_scale_factor)
            .await
            .context("cannot get batch fee input from base provider")?)
    }

    /// Returns the fee model parameters.
    fn get_fee_model_params(&self) -> FeeParams {
        self.inner.get_fee_model_params()
    }
}

/// Mock [`BatchFeeModelInputProvider`] implementation that returns a constant value.
/// Intended to be used in tests only.
#[derive(Debug)]
pub struct MockBatchFeeParamsProvider(pub FeeParams);

impl Default for MockBatchFeeParamsProvider {
    fn default() -> Self {
        Self(FeeParams::sensible_v1_default())
    }
}

#[async_trait]
impl BatchFeeModelInputProvider for MockBatchFeeParamsProvider {
    fn get_fee_model_params(&self) -> FeeParams {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use l1_gas_price::GasAdjusterClient;
    use zksync_config::GasAdjusterConfig;
    use zksync_eth_client::{clients::MockSettlementLayer, BaseFees};
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_node_test_utils::create_l1_batch;
    use zksync_types::{
        commitment::L1BatchCommitmentMode,
        eth_sender::EthTxFinalityStatus,
        fee_model::{BaseTokenConversionRatio, FeeModelConfigV2},
        pubdata_da::PubdataSendingMode,
        U256,
    };
    use zksync_web3_decl::client::{DynClient, L2};

    use super::*;

    #[derive(Debug, Clone)]
    struct DummyTokenRatioProvider {
        ratio: BaseTokenConversionRatio,
    }

    impl DummyTokenRatioProvider {
        pub fn new(ratio: BaseTokenConversionRatio) -> Self {
            Self { ratio }
        }
    }

    #[async_trait]
    impl BaseTokenRatioProvider for DummyTokenRatioProvider {
        fn get_conversion_ratio(&self) -> BaseTokenConversionRatio {
            self.ratio
        }
    }

    #[tokio::test]
    async fn test_get_fee_model_params() {
        struct TestCase {
            name: &'static str,
            conversion_ratio: BaseTokenConversionRatio,
            input_minimal_l2_gas_price: u64,    // Wei denomination
            input_l1_gas_price: u64,            // Wei
            input_l1_pubdata_price: u64,        // Wei
            expected_minimal_l2_gas_price: u64, // BaseToken denomination
            expected_l1_gas_price: u64,         // BaseToken
            expected_l1_pubdata_price: u64,     // BaseToken
        }
        let test_cases = vec![
            TestCase {
                name: "1 ETH = 2 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(2).unwrap(),
                    denominator: NonZeroU64::new(1).unwrap(),
                },
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 2000,
                expected_l1_gas_price: 4000,
                expected_l1_pubdata_price: 6000,
            },
            TestCase {
                name: "1 ETH = 0.5 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1).unwrap(),
                    denominator: NonZeroU64::new(2).unwrap(),
                },
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 500,
                expected_l1_gas_price: 1000,
                expected_l1_pubdata_price: 1500,
            },
            TestCase {
                name: "1 ETH = 1 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1).unwrap(),
                    denominator: NonZeroU64::new(1).unwrap(),
                },
                input_minimal_l2_gas_price: 1000,
                input_l1_gas_price: 2000,
                input_l1_pubdata_price: 3000,
                expected_minimal_l2_gas_price: 1000,
                expected_l1_gas_price: 2000,
                expected_l1_pubdata_price: 3000,
            },
            TestCase {
                name: "Large conversion - 1 ETH = 1_000_000 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1_000_000).unwrap(),
                    denominator: NonZeroU64::new(1).unwrap(),
                },
                input_minimal_l2_gas_price: 1_000_000,
                input_l1_gas_price: 2_000_000,
                input_l1_pubdata_price: 3_000_000,
                expected_minimal_l2_gas_price: 1_000_000_000_000,
                expected_l1_gas_price: 2_000_000_000_000,
                expected_l1_pubdata_price: 3_000_000_000_000,
            },
            TestCase {
                name: "Small conversion - 1 ETH = 0.001 BaseToken",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1).unwrap(),
                    denominator: NonZeroU64::new(1_000).unwrap(),
                },
                input_minimal_l2_gas_price: 1_000_000,
                input_l1_gas_price: 2_000_000,
                input_l1_pubdata_price: 3_000_000,
                expected_minimal_l2_gas_price: 1_000,
                expected_l1_gas_price: 2_000,
                expected_l1_pubdata_price: 3_000,
            },
            TestCase {
                name: "Fractional conversion ratio 123456789",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(1123456789).unwrap(),
                    denominator: NonZeroU64::new(1_000_000_000).unwrap(),
                },
                input_minimal_l2_gas_price: 1_000_000,
                input_l1_gas_price: 2_000_000,
                input_l1_pubdata_price: 3_000_000,
                expected_minimal_l2_gas_price: 1123456,
                expected_l1_gas_price: 2246913,
                expected_l1_pubdata_price: 3370370,
            },
            TestCase {
                name: "Conversion ratio too large so clamp down to u64::MAX",
                conversion_ratio: BaseTokenConversionRatio {
                    numerator: NonZeroU64::new(u64::MAX).unwrap(),
                    denominator: NonZeroU64::new(1).unwrap(),
                },
                input_minimal_l2_gas_price: 2,
                input_l1_gas_price: 2,
                input_l1_pubdata_price: 2,
                expected_minimal_l2_gas_price: u64::MAX,
                expected_l1_gas_price: u64::MAX,
                expected_l1_pubdata_price: u64::MAX,
            },
        ];

        for case in test_cases {
            let gas_adjuster =
                setup_gas_adjuster(case.input_l1_gas_price, case.input_l1_pubdata_price).await;

            let base_token_ratio_provider = DummyTokenRatioProvider::new(case.conversion_ratio);

            let config = FeeModelConfig::V2(FeeModelConfigV2 {
                minimal_l2_gas_price: case.input_minimal_l2_gas_price,
                compute_overhead_part: 1.0,
                pubdata_overhead_part: 1.0,
                batch_overhead_l1_gas: 1,
                max_gas_per_batch: 1,
                max_pubdata_per_batch: 1,
            });

            let fee_provider = MainNodeFeeInputProvider::new(
                Arc::new(gas_adjuster),
                Arc::new(base_token_ratio_provider),
                config,
            );

            let fee_params = fee_provider.get_fee_model_params();

            if let FeeParams::V2(params) = fee_params {
                assert_eq!(
                    params.l1_gas_price(),
                    case.expected_l1_gas_price,
                    "Test case '{}' failed: l1_gas_price mismatch",
                    case.name
                );
                assert_eq!(
                    params.l1_pubdata_price(),
                    case.expected_l1_pubdata_price,
                    "Test case '{}' failed: l1_pubdata_price mismatch",
                    case.name
                );
                assert_eq!(
                    params.config().minimal_l2_gas_price,
                    case.expected_minimal_l2_gas_price,
                    "Test case '{}' failed: minimal_l2_gas_price mismatch",
                    case.name
                );
            } else {
                panic!("Expected FeeParams::V2 for test case '{}'", case.name);
            }
        }
    }

    // Helper function to create BaseFees.
    fn test_base_fees(block: u64, blob: U256, pubdata: U256) -> BaseFees {
        BaseFees {
            base_fee_per_gas: block,
            base_fee_per_blob_gas: blob,
            l2_pubdata_price: pubdata,
        }
    }

    // Helper function to setup the GasAdjuster.
    async fn setup_gas_adjuster(l1_gas_price: u64, l1_pubdata_price: u64) -> GasAdjuster {
        let mock = MockSettlementLayer::builder()
            .with_fee_history(vec![
                test_base_fees(0, U256::from(4), U256::from(0)),
                test_base_fees(1, U256::from(3), U256::from(0)),
            ])
            .build();
        mock.advance_block_number(2, EthTxFinalityStatus::Finalized); // Ensure we have enough blocks for the fee history

        let gas_adjuster_config = GasAdjusterConfig {
            internal_enforced_l1_gas_price: Some(l1_gas_price),
            internal_enforced_pubdata_price: Some(l1_pubdata_price),
            max_base_fee_samples: 1, // Ensure this is less than the number of blocks
            num_samples_for_blob_base_fee_estimate: 2,
            ..Default::default()
        };

        let client: Box<DynClient<L2>> = Box::new(mock.clone().into_client());

        GasAdjuster::new(
            GasAdjusterClient::from(client),
            gas_adjuster_config,
            PubdataSendingMode::Blobs,
            L1BatchCommitmentMode::Rollup,
        )
        .await
        .expect("Failed to create GasAdjuster")
    }

    #[tokio::test]
    async fn test_take_fee_input_from_unsealed_batch() {
        let sealed_batch_fee_input = BatchFeeInput::pubdata_independent(1, 2, 3);
        let unsealed_batch_fee_input = BatchFeeInput::pubdata_independent(101, 102, 103);

        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        insert_genesis_batch(&mut conn, &GenesisParams::mock())
            .await
            .unwrap();

        let mut l1_batch_header = create_l1_batch(1);
        l1_batch_header.batch_fee_input = sealed_batch_fee_input;
        conn.blocks_dal()
            .insert_mock_l1_batch(&l1_batch_header)
            .await
            .unwrap();
        let mut l1_batch_header = create_l1_batch(2);
        l1_batch_header.batch_fee_input = unsealed_batch_fee_input;
        conn.blocks_dal()
            .insert_l1_batch(l1_batch_header.to_unsealed_header())
            .await
            .unwrap();
        let provider: &dyn BatchFeeModelInputProvider =
            &ApiFeeInputProvider::new(Arc::new(MockBatchFeeParamsProvider::default()), pool);
        let fee_input = provider.get_batch_fee_input().await.unwrap();
        assert_eq!(fee_input, unsealed_batch_fee_input);
    }
}
