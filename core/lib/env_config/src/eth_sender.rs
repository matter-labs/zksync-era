use anyhow::Context as _;
use zksync_config::{
    configs::eth_sender::SenderConfig, EthConfig, EthWatchConfig, GasAdjusterConfig,
};

use crate::{envy_load, FromEnv};

impl FromEnv for EthConfig {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            sender: SenderConfig::from_env().ok(),
            gas_adjuster: GasAdjusterConfig::from_env().ok(),
            watcher: EthWatchConfig::from_env().ok(),
            web3_url: std::env::var("ETH_CLIENT_WEB3_URL").context("ETH_CLIENT_WEB3_URL")?,
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
    use zksync_config::configs::eth_sender::{
        ProofLoadingMode, ProofSendingMode, PubdataSendingMode,
    };

    use super::*;
    use crate::test_utils::{hash, EnvMutex};

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> EthConfig {
        EthConfig {
            sender: Some(SenderConfig {
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
                pubdata_sending_mode: PubdataSendingMode::Calldata,
            }),
            gas_adjuster: Some(GasAdjusterConfig {
                default_priority_fee_per_gas: 20000000000,
                max_base_fee_samples: 10000,
                pricing_formula_parameter_a: 1.5,
                pricing_formula_parameter_b: 1.0005,
                internal_l1_pricing_multiplier: 0.8,
                internal_enforced_l1_gas_price: None,
                internal_enforced_pubdata_price: None,
                poll_period: 15,
                max_l1_gas_price: Some(100000000),
                num_samples_for_blob_base_fee_estimate: 10,
                internal_pubdata_pricing_multiplier: 1.0,
                max_blob_base_fee: None,
            }),
            watcher: Some(EthWatchConfig {
                confirmations_for_eth_event: Some(0),
                eth_node_poll_interval: 300,
            }),
            web3_url: "http://127.0.0.1:8545".to_string(),
        }
    }

    #[test]
    #[allow(deprecated)]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            ETH_WATCH_CONFIRMATIONS_FOR_ETH_EVENT = "0"
            ETH_WATCH_ETH_NODE_POLL_INTERVAL = "30"
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
            ETH_SENDER_GAS_ADJUSTER_MAX_BLOB_BASE_FEE_SAMPLES="10"
            ETH_SENDER_GAS_ADJUSTER_INTERNAL_PUBDATA_PRICING_MULTIPLIER="1.0"
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
            ETH_SENDER_SENDER_PUBDATA_SENDING_MODE="Calldata"
            ETH_CLIENT_WEB3_URL="http://127.0.0.1:8545"

        "#;
        lock.set_env(config);

        let actual = EthConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
        let private_key = actual
            .sender
            .unwrap()
            .private_key()
            .unwrap()
            .expect("no private key");
        assert_eq!(
            private_key.expose_secret().secret_bytes(),
            hash("27593fea79697e947890ecbecce7901b0008345e5d7259710d0dd5e500d040be").as_bytes()
        );
    }
}
