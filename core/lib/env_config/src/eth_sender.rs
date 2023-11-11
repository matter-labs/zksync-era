use anyhow::Context as _;
use zksync_config::{configs::eth_sender::SenderConfig, ETHSenderConfig, GasAdjusterConfig};

use crate::{envy_load, FromEnv};

impl FromEnv for ETHSenderConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            sender: SenderConfig::from_env().context("SenderConfig")?,
            gas_adjuster: GasAdjusterConfig::from_env().context("GasAdjusterConfig")?,
        })
    }
}

impl FromEnv for SenderConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("eth_sender", "ETH_SENDER_SENDER_")
    }
}

impl FromEnv for GasAdjusterConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("eth_sender.gas_adjuster", "ETH_SENDER_GAS_ADJUSTER_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::eth_sender::{ProofLoadingMode, ProofSendingMode};

    use super::*;
    use crate::test_utils::{hash, EnvMutex};

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ETHSenderConfig {
        ETHSenderConfig {
            sender: SenderConfig {
                aggregated_proof_sizes: vec![1, 5],
                aggregated_block_commit_deadline: 30,
                aggregated_block_prove_deadline: 3_000,
                aggregated_block_execute_deadline: 4_000,
                max_aggregated_tx_gas: 4_000_000,
                max_eth_tx_data_size: 120_000,

                timestamp_criteria_max_allowed_lag: 30,
                max_aggregated_blocks_to_commit: 3,
                max_aggregated_blocks_to_execute: 4,
                wait_confirmations: Some(1),
                tx_poll_period: 3,
                aggregate_tx_poll_period: 3,
                max_txs_in_flight: 3,
                proof_sending_mode: ProofSendingMode::SkipEveryProof,
                l1_batch_min_age_before_execute_seconds: Some(1000),
                max_acceptable_priority_fee_in_gwei: 100_000_000_000,
                proof_loading_mode: ProofLoadingMode::OldProofFromDb,
            },
            gas_adjuster: GasAdjusterConfig {
                default_priority_fee_per_gas: 20000000000,
                max_base_fee_samples: 10000,
                pricing_formula_parameter_a: 1.5,
                pricing_formula_parameter_b: 1.0005,
                internal_l1_pricing_multiplier: 0.8,
                internal_enforced_l1_gas_price: None,
                poll_period: 15,
                max_l1_gas_price: Some(100000000),
            },
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            ETH_SENDER_SENDER_WAIT_CONFIRMATIONS="1"
            ETH_SENDER_SENDER_TX_POLL_PERIOD="3"
            ETH_SENDER_SENDER_AGGREGATE_TX_POLL_PERIOD="3"
            ETH_SENDER_SENDER_MAX_TXS_IN_FLIGHT="3"
            ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY="0x27593fea79697e947890ecbecce7901b0008345e5d7259710d0dd5e500d040be"
            ETH_SENDER_SENDER_PROOF_SENDING_MODE="SkipEveryProof"
            ETH_SENDER_GAS_ADJUSTER_DEFAULT_PRIORITY_FEE_PER_GAS="20000000000"
            ETH_SENDER_GAS_ADJUSTER_MAX_BASE_FEE_SAMPLES="10000"
            ETH_SENDER_GAS_ADJUSTER_PRICING_FORMULA_PARAMETER_A="1.5"
            ETH_SENDER_GAS_ADJUSTER_PRICING_FORMULA_PARAMETER_B="1.0005"
            ETH_SENDER_GAS_ADJUSTER_INTERNAL_L1_PRICING_MULTIPLIER="0.8"
            ETH_SENDER_GAS_ADJUSTER_POLL_PERIOD="15"
            ETH_SENDER_GAS_ADJUSTER_MAX_L1_GAS_PRICE="100000000"
            ETH_SENDER_WAIT_FOR_PROOFS="false"
            ETH_SENDER_SENDER_AGGREGATED_PROOF_SIZES="1,5"
            ETH_SENDER_SENDER_MAX_AGGREGATED_BLOCKS_TO_COMMIT="3"
            ETH_SENDER_SENDER_MAX_AGGREGATED_BLOCKS_TO_EXECUTE="4"
            ETH_SENDER_SENDER_AGGREGATED_BLOCK_COMMIT_DEADLINE="30"
            ETH_SENDER_SENDER_AGGREGATED_BLOCK_PROVE_DEADLINE="3000"
            ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE="4000"
            ETH_SENDER_SENDER_TIMESTAMP_CRITERIA_MAX_ALLOWED_LAG="30"
            ETH_SENDER_SENDER_MAX_AGGREGATED_TX_GAS="4000000"
            ETH_SENDER_SENDER_MAX_ETH_TX_DATA_SIZE="120000"
            ETH_SENDER_SENDER_L1_BATCH_MIN_AGE_BEFORE_EXECUTE_SECONDS="1000"
            ETH_SENDER_SENDER_MAX_ACCEPTABLE_PRIORITY_FEE_IN_GWEI="100000000000"
            ETH_SENDER_SENDER_PROOF_LOADING_MODE="OldProofFromDb"
        "#;
        lock.set_env(config);

        let actual = ETHSenderConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
        assert_eq!(
            actual.sender.private_key().unwrap(),
            hash("27593fea79697e947890ecbecce7901b0008345e5d7259710d0dd5e500d040be")
        );
    }
}
