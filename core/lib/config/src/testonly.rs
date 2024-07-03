use std::num::NonZeroUsize;

use rand::{distributions::Distribution, Rng};
use zksync_basic_types::{
    basic_fri_types::CircuitIdRoundTuple,
    commitment::L1BatchCommitmentMode,
    network::Network,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    L1BatchNumber, L1ChainId, L2ChainId,
};
use zksync_consensus_utils::EncodeDist;
use zksync_crypto_primitives::K256PrivateKey;

use crate::configs::{self, eth_sender::PubdataSendingMode};

trait Sample {
    fn sample(rng: &mut (impl Rng + ?Sized)) -> Self;
}

impl Sample for Network {
    fn sample(rng: &mut (impl Rng + ?Sized)) -> Network {
        type T = Network;
        match rng.gen_range(0..8) {
            0 => T::Mainnet,
            1 => T::Rinkeby,
            2 => T::Ropsten,
            3 => T::Goerli,
            4 => T::Sepolia,
            5 => T::Localhost,
            6 => T::Unknown,
            _ => T::Test,
        }
    }
}

impl Distribution<configs::chain::FeeModelVersion> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::chain::FeeModelVersion {
        type T = configs::chain::FeeModelVersion;
        match rng.gen_range(0..2) {
            0 => T::V1,
            _ => T::V2,
        }
    }
}

impl Distribution<configs::ApiConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ApiConfig {
        configs::ApiConfig {
            web3_json_rpc: self.sample(rng),
            prometheus: self.sample(rng),
            healthcheck: self.sample(rng),
            merkle_tree: self.sample(rng),
        }
    }
}

impl Distribution<configs::api::Web3JsonRpcConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::api::Web3JsonRpcConfig {
        configs::api::Web3JsonRpcConfig {
            http_port: self.sample(rng),
            http_url: self.sample(rng),
            ws_port: self.sample(rng),
            ws_url: self.sample(rng),
            req_entities_limit: self.sample(rng),
            filters_disabled: self.sample(rng),
            filters_limit: self.sample(rng),
            subscriptions_limit: self.sample(rng),
            pubsub_polling_interval: self.sample(rng),
            max_nonce_ahead: self.sample(rng),
            gas_price_scale_factor: self.sample(rng),
            request_timeout: self.sample_opt(|| self.sample(rng)),
            account_pks: self.sample_opt(|| self.sample_range(rng).map(|_| rng.gen()).collect()),
            estimate_gas_scale_factor: self.sample(rng),
            estimate_gas_acceptable_overestimation: self.sample(rng),
            max_tx_size: self.sample(rng),
            vm_execution_cache_misses_limit: self.sample(rng),
            vm_concurrency_limit: self.sample(rng),
            factory_deps_cache_size_mb: self.sample(rng),
            initial_writes_cache_size_mb: self.sample(rng),
            latest_values_cache_size_mb: self.sample(rng),
            fee_history_limit: self.sample(rng),
            max_batch_request_size: self.sample(rng),
            max_response_body_size_mb: self.sample(rng),
            max_response_body_size_overrides_mb: [
                (
                    "eth_call",
                    NonZeroUsize::new(self.sample(rng)).unwrap_or(NonZeroUsize::MAX),
                ),
                (
                    "zks_getProof",
                    NonZeroUsize::new(self.sample(rng)).unwrap_or(NonZeroUsize::MAX),
                ),
            ]
            .into_iter()
            .collect(),
            websocket_requests_per_minute_limit: self.sample(rng),
            tree_api_url: self.sample(rng),
            mempool_cache_update_interval: self.sample(rng),
            mempool_cache_size: self.sample(rng),
            whitelisted_tokens_for_aa: self.sample_range(rng).map(|_| rng.gen()).collect(),
            api_namespaces: self
                .sample_opt(|| self.sample_range(rng).map(|_| self.sample(rng)).collect()),
            extended_api_tracing: self.sample(rng),
        }
    }
}

impl Distribution<configs::api::HealthCheckConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::api::HealthCheckConfig {
        configs::api::HealthCheckConfig {
            port: self.sample(rng),
            slow_time_limit_ms: self.sample(rng),
            hard_time_limit_ms: self.sample(rng),
        }
    }
}

impl Distribution<configs::api::ContractVerificationApiConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::api::ContractVerificationApiConfig {
        configs::api::ContractVerificationApiConfig {
            port: self.sample(rng),
            url: self.sample(rng),
        }
    }
}

impl Distribution<configs::api::MerkleTreeApiConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::api::MerkleTreeApiConfig {
        configs::api::MerkleTreeApiConfig {
            port: self.sample(rng),
        }
    }
}

impl Distribution<configs::PrometheusConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::PrometheusConfig {
        configs::PrometheusConfig {
            listener_port: self.sample(rng),
            pushgateway_url: self.sample(rng),
            push_interval_ms: self.sample(rng),
        }
    }
}

impl Distribution<configs::chain::NetworkConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::chain::NetworkConfig {
        configs::chain::NetworkConfig {
            network: Sample::sample(rng),
            zksync_network: self.sample(rng),
            zksync_network_id: L2ChainId::max(),
        }
    }
}

impl Distribution<configs::chain::StateKeeperConfig> for EncodeDist {
    #[allow(deprecated)]
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::chain::StateKeeperConfig {
        configs::chain::StateKeeperConfig {
            transaction_slots: self.sample(rng),
            block_commit_deadline_ms: self.sample(rng),
            l2_block_commit_deadline_ms: self.sample(rng),
            l2_block_seal_queue_capacity: self.sample(rng),
            l2_block_max_payload_size: self.sample(rng),
            max_single_tx_gas: self.sample(rng),
            max_allowed_l2_tx_gas_limit: self.sample(rng),
            reject_tx_at_geometry_percentage: self.sample(rng),
            reject_tx_at_eth_params_percentage: self.sample(rng),
            reject_tx_at_gas_percentage: self.sample(rng),
            close_block_at_geometry_percentage: self.sample(rng),
            close_block_at_eth_params_percentage: self.sample(rng),
            close_block_at_gas_percentage: self.sample(rng),
            minimal_l2_gas_price: self.sample(rng),
            compute_overhead_part: self.sample(rng),
            pubdata_overhead_part: self.sample(rng),
            batch_overhead_l1_gas: self.sample(rng),
            max_gas_per_batch: self.sample(rng),
            max_pubdata_per_batch: self.sample(rng),
            fee_model_version: self.sample(rng),
            validation_computational_gas_limit: self.sample(rng),
            save_call_traces: self.sample(rng),
            max_circuits_per_batch: self.sample(rng),
            protective_reads_persistence_enabled: self.sample(rng),
            // These values are not involved into files serialization skip them
            fee_account_addr: None,
            bootloader_hash: None,
            default_aa_hash: None,
            l1_batch_commit_data_generator_mode: Default::default(),
        }
    }
}

impl Distribution<configs::chain::OperationsManagerConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::chain::OperationsManagerConfig {
        configs::chain::OperationsManagerConfig {
            delay_interval: self.sample(rng),
        }
    }
}

impl Distribution<configs::chain::CircuitBreakerConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::chain::CircuitBreakerConfig {
        configs::chain::CircuitBreakerConfig {
            sync_interval_ms: self.sample(rng),
            http_req_max_retry_number: self.sample(rng),
            http_req_retry_interval_sec: self.sample(rng),
            replication_lag_limit_sec: self.sample(rng),
        }
    }
}

impl Distribution<configs::chain::MempoolConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::chain::MempoolConfig {
        configs::chain::MempoolConfig {
            sync_interval_ms: self.sample(rng),
            sync_batch_size: self.sample(rng),
            capacity: self.sample(rng),
            stuck_tx_timeout: self.sample(rng),
            remove_stuck_txs: self.sample(rng),
            delay_interval: self.sample(rng),
        }
    }
}

impl Distribution<configs::ContractVerifierConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ContractVerifierConfig {
        configs::ContractVerifierConfig {
            compilation_timeout: self.sample(rng),
            polling_interval: self.sample(rng),
            prometheus_port: self.sample(rng),
            threads_per_server: self.sample(rng),
            port: self.sample(rng),
            url: self.sample(rng),
        }
    }
}

impl Distribution<configs::ContractsConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, g: &mut R) -> configs::ContractsConfig {
        configs::ContractsConfig {
            governance_addr: g.gen(),
            verifier_addr: g.gen(),
            default_upgrade_addr: g.gen(),
            diamond_proxy_addr: g.gen(),
            validator_timelock_addr: g.gen(),
            l1_erc20_bridge_proxy_addr: g.gen(),
            l2_erc20_bridge_addr: g.gen(),
            l1_shared_bridge_proxy_addr: g.gen(),
            l2_shared_bridge_addr: g.gen(),
            l1_weth_bridge_proxy_addr: g.gen(),
            l2_weth_bridge_addr: g.gen(),
            l2_testnet_paymaster_addr: g.gen(),
            l1_multicall3_addr: g.gen(),
            base_token_addr: g.gen(),
            ecosystem_contracts: self.sample(g),
        }
    }
}

impl Distribution<configs::database::MerkleTreeMode> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::database::MerkleTreeMode {
        type T = configs::database::MerkleTreeMode;
        match rng.gen_range(0..2) {
            0 => T::Full,
            _ => T::Lightweight,
        }
    }
}

impl Distribution<configs::database::MerkleTreeConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::database::MerkleTreeConfig {
        configs::database::MerkleTreeConfig {
            path: self.sample(rng),
            mode: self.sample(rng),
            multi_get_chunk_size: self.sample(rng),
            block_cache_size_mb: self.sample(rng),
            memtable_capacity_mb: self.sample(rng),
            stalled_writes_timeout_sec: self.sample(rng),
            max_l1_batches_per_iter: self.sample(rng),
        }
    }
}

impl Distribution<configs::ExperimentalDBConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ExperimentalDBConfig {
        configs::ExperimentalDBConfig {
            state_keeper_db_block_cache_capacity_mb: self.sample(rng),
            state_keeper_db_max_open_files: self.sample(rng),
            protective_reads_persistence_enabled: self.sample(rng),
            processing_delay_ms: self.sample(rng),
            include_indices_and_filters_in_block_cache: self.sample(rng),
        }
    }
}

impl Distribution<configs::database::DBConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::database::DBConfig {
        configs::database::DBConfig {
            state_keeper_db_path: self.sample(rng),
            merkle_tree: self.sample(rng),
            experimental: self.sample(rng),
        }
    }
}

impl Distribution<configs::database::PostgresConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::database::PostgresConfig {
        configs::database::PostgresConfig {
            max_connections: self.sample(rng),
            max_connections_master: self.sample(rng),
            acquire_timeout_sec: self.sample(rng),
            statement_timeout_sec: self.sample(rng),
            long_connection_threshold_ms: self.sample(rng),
            slow_query_threshold_ms: self.sample(rng),
            test_server_url: self.sample(rng),
            test_prover_url: self.sample(rng),
        }
    }
}

impl Distribution<configs::EthConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::EthConfig {
        configs::EthConfig {
            sender: self.sample(rng),
            gas_adjuster: self.sample(rng),
            watcher: self.sample(rng),
        }
    }
}

impl Distribution<configs::eth_sender::ProofSendingMode> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::eth_sender::ProofSendingMode {
        type T = configs::eth_sender::ProofSendingMode;
        match rng.gen_range(0..3) {
            0 => T::OnlyRealProofs,
            1 => T::OnlySampledProofs,
            _ => T::SkipEveryProof,
        }
    }
}

impl Distribution<configs::eth_sender::ProofLoadingMode> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::eth_sender::ProofLoadingMode {
        type T = configs::eth_sender::ProofLoadingMode;
        match rng.gen_range(0..2) {
            0 => T::OldProofFromDb,
            _ => T::FriProofFromGcs,
        }
    }
}

impl Distribution<configs::eth_sender::PubdataSendingMode> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::eth_sender::PubdataSendingMode {
        type T = configs::eth_sender::PubdataSendingMode;
        match rng.gen_range(0..2) {
            0 => T::Calldata,
            _ => T::Blobs,
        }
    }
}

impl Distribution<configs::eth_sender::SenderConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::eth_sender::SenderConfig {
        configs::eth_sender::SenderConfig {
            aggregated_proof_sizes: self.sample_collect(rng),
            wait_confirmations: self.sample(rng),
            tx_poll_period: self.sample(rng),
            aggregate_tx_poll_period: self.sample(rng),
            max_txs_in_flight: self.sample(rng),
            proof_sending_mode: self.sample(rng),
            max_aggregated_tx_gas: self.sample(rng),
            max_eth_tx_data_size: self.sample(rng),
            max_aggregated_blocks_to_commit: self.sample(rng),
            max_aggregated_blocks_to_execute: self.sample(rng),
            aggregated_block_commit_deadline: self.sample(rng),
            aggregated_block_prove_deadline: self.sample(rng),
            aggregated_block_execute_deadline: self.sample(rng),
            timestamp_criteria_max_allowed_lag: self.sample(rng),
            l1_batch_min_age_before_execute_seconds: self.sample(rng),
            max_acceptable_priority_fee_in_gwei: self.sample(rng),
            pubdata_sending_mode: PubdataSendingMode::Calldata,
        }
    }
}

impl Distribution<configs::eth_sender::GasAdjusterConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::eth_sender::GasAdjusterConfig {
        configs::eth_sender::GasAdjusterConfig {
            default_priority_fee_per_gas: self.sample(rng),
            max_base_fee_samples: self.sample(rng),
            pricing_formula_parameter_a: self.sample(rng),
            pricing_formula_parameter_b: self.sample(rng),
            internal_l1_pricing_multiplier: self.sample(rng),
            internal_enforced_l1_gas_price: self.sample(rng),
            internal_enforced_pubdata_price: self.sample(rng),
            poll_period: self.sample(rng),
            max_l1_gas_price: self.sample(rng),
            num_samples_for_blob_base_fee_estimate: self.sample(rng),
            internal_pubdata_pricing_multiplier: self.sample(rng),
            max_blob_base_fee: self.sample(rng),
        }
    }
}

impl Distribution<configs::EthWatchConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::EthWatchConfig {
        configs::EthWatchConfig {
            confirmations_for_eth_event: self.sample(rng),
            eth_node_poll_interval: self.sample(rng),
        }
    }
}

impl Distribution<configs::FriProofCompressorConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::FriProofCompressorConfig {
        configs::FriProofCompressorConfig {
            compression_mode: self.sample(rng),
            prometheus_listener_port: self.sample(rng),
            prometheus_pushgateway_url: self.sample(rng),
            prometheus_push_interval_ms: self.sample(rng),
            generation_timeout_in_secs: self.sample(rng),
            max_attempts: self.sample(rng),
            universal_setup_path: self.sample(rng),
            universal_setup_download_url: self.sample(rng),
            verify_wrapper_proof: self.sample(rng),
        }
    }
}

impl Distribution<configs::fri_prover::SetupLoadMode> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::fri_prover::SetupLoadMode {
        type T = configs::fri_prover::SetupLoadMode;
        match rng.gen_range(0..2) {
            0 => T::FromDisk,
            _ => T::FromMemory,
        }
    }
}

impl Distribution<configs::FriProverConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::FriProverConfig {
        configs::FriProverConfig {
            setup_data_path: self.sample(rng),
            prometheus_port: self.sample(rng),
            max_attempts: self.sample(rng),
            generation_timeout_in_secs: self.sample(rng),
            setup_load_mode: self.sample(rng),
            specialized_group_id: self.sample(rng),
            queue_capacity: self.sample(rng),
            witness_vector_receiver_port: self.sample(rng),
            zone_read_url: self.sample(rng),
            shall_save_to_public_bucket: self.sample(rng),
            availability_check_interval_in_secs: self.sample(rng),
            prover_object_store: self.sample(rng),
            public_object_store: self.sample(rng),
        }
    }
}

impl Distribution<configs::FriProverGatewayConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::FriProverGatewayConfig {
        configs::FriProverGatewayConfig {
            api_url: self.sample(rng),
            api_poll_duration_secs: self.sample(rng),
            prometheus_listener_port: self.sample(rng),
            prometheus_pushgateway_url: self.sample(rng),
            prometheus_push_interval_ms: self.sample(rng),
        }
    }
}

impl Sample for CircuitIdRoundTuple {
    fn sample(rng: &mut (impl Rng + ?Sized)) -> CircuitIdRoundTuple {
        CircuitIdRoundTuple {
            circuit_id: rng.gen(),
            aggregation_round: rng.gen(),
        }
    }
}

impl Distribution<configs::fri_prover_group::FriProverGroupConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::fri_prover_group::FriProverGroupConfig {
        configs::fri_prover_group::FriProverGroupConfig {
            group_0: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_1: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_2: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_3: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_4: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_5: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_6: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_7: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_8: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_9: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_10: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_11: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_12: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_13: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
            group_14: self
                .sample_range(rng)
                .map(|_| Sample::sample(rng))
                .collect(),
        }
    }
}

impl Distribution<configs::FriWitnessGeneratorConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::FriWitnessGeneratorConfig {
        configs::FriWitnessGeneratorConfig {
            generation_timeout_in_secs: self.sample(rng),
            basic_generation_timeout_in_secs: self.sample(rng),
            leaf_generation_timeout_in_secs: self.sample(rng),
            node_generation_timeout_in_secs: self.sample(rng),
            recursion_tip_generation_timeout_in_secs: self.sample(rng),
            scheduler_generation_timeout_in_secs: self.sample(rng),
            max_attempts: self.sample(rng),
            last_l1_batch_to_process: self.sample(rng),
            shall_save_to_public_bucket: self.sample(rng),
        }
    }
}

impl Distribution<configs::FriWitnessVectorGeneratorConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::FriWitnessVectorGeneratorConfig {
        configs::FriWitnessVectorGeneratorConfig {
            max_prover_reservation_duration_in_secs: self.sample(rng),
            prover_instance_wait_timeout_in_secs: self.sample(rng),
            prover_instance_poll_time_in_milli_secs: self.sample(rng),
            prometheus_listener_port: self.sample(rng),
            prometheus_pushgateway_url: self.sample(rng),
            prometheus_push_interval_ms: self.sample(rng),
            specialized_group_id: self.sample(rng),
        }
    }
}

impl Distribution<configs::house_keeper::HouseKeeperConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::house_keeper::HouseKeeperConfig {
        configs::house_keeper::HouseKeeperConfig {
            l1_batch_metrics_reporting_interval_ms: self.sample(rng),
            gpu_prover_queue_reporting_interval_ms: self.sample(rng),
            prover_job_retrying_interval_ms: self.sample(rng),
            prover_stats_reporting_interval_ms: self.sample(rng),
            witness_job_moving_interval_ms: self.sample(rng),
            witness_generator_stats_reporting_interval_ms: self.sample(rng),
            prover_db_pool_size: self.sample(rng),
            witness_generator_job_retrying_interval_ms: self.sample(rng),
            proof_compressor_job_retrying_interval_ms: self.sample(rng),
            proof_compressor_stats_reporting_interval_ms: self.sample(rng),
            prover_job_archiver_archiving_interval_ms: self.sample(rng),
            prover_job_archiver_archive_after_secs: self.sample(rng),
            fri_gpu_prover_archiver_archiving_interval_ms: self.sample(rng),
            fri_gpu_prover_archiver_archive_after_secs: self.sample(rng),
        }
    }
}

impl Distribution<configs::object_store::ObjectStoreMode> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::object_store::ObjectStoreMode {
        type T = configs::object_store::ObjectStoreMode;
        match rng.gen_range(0..4) {
            0 => T::GCS {
                bucket_base_url: self.sample(rng),
            },
            1 => T::GCSWithCredentialFile {
                bucket_base_url: self.sample(rng),
                gcs_credential_file_path: self.sample(rng),
            },
            2 => T::FileBacked {
                file_backed_base_path: self.sample(rng),
            },
            _ => T::GCSAnonymousReadOnly {
                bucket_base_url: self.sample(rng),
            },
        }
    }
}

impl Distribution<configs::ObjectStoreConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ObjectStoreConfig {
        configs::ObjectStoreConfig {
            mode: self.sample(rng),
            max_retries: self.sample(rng),
            local_mirror_path: self.sample(rng),
        }
    }
}

impl Distribution<configs::ProofDataHandlerConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ProofDataHandlerConfig {
        configs::ProofDataHandlerConfig {
            http_port: self.sample(rng),
            proof_generation_timeout_in_secs: self.sample(rng),
            tee_support: self.sample(rng),
        }
    }
}

impl Distribution<configs::SnapshotsCreatorConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::SnapshotsCreatorConfig {
        configs::SnapshotsCreatorConfig {
            l1_batch_number: self.sample_opt(|| L1BatchNumber(rng.gen())),
            version: if rng.gen() { 0 } else { 1 },
            storage_logs_chunk_size: self.sample(rng),
            concurrent_queries_count: self.sample(rng),
            object_store: self.sample(rng),
        }
    }
}

impl Distribution<configs::ObservabilityConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ObservabilityConfig {
        configs::ObservabilityConfig {
            sentry_url: self.sample(rng),
            sentry_environment: self.sample(rng),
            log_format: self.sample(rng),
            opentelemetry: self.sample(rng),
            log_directives: self.sample(rng),
        }
    }
}

impl Distribution<configs::OpentelemetryConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::OpentelemetryConfig {
        configs::OpentelemetryConfig {
            level: self.sample(rng),
            endpoint: self.sample(rng),
        }
    }
}

impl Distribution<configs::GenesisConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::GenesisConfig {
        configs::GenesisConfig {
            protocol_version: Some(ProtocolSemanticVersion {
                minor: ProtocolVersionId::try_from(
                    rng.gen_range(0..(ProtocolVersionId::latest() as u16)),
                )
                .unwrap(),
                patch: VersionPatch(rng.gen()),
            }),
            genesis_root_hash: Some(rng.gen()),
            rollup_last_leaf_index: Some(self.sample(rng)),
            genesis_commitment: Some(rng.gen()),
            bootloader_hash: Some(rng.gen()),
            default_aa_hash: Some(rng.gen()),
            fee_account: rng.gen(),
            l1_chain_id: L1ChainId(self.sample(rng)),
            l2_chain_id: L2ChainId::default(),
            recursion_node_level_vk_hash: rng.gen(),
            recursion_leaf_level_vk_hash: rng.gen(),
            recursion_scheduler_level_vk_hash: rng.gen(),
            recursion_circuits_set_vks_hash: rng.gen(),
            dummy_verifier: rng.gen(),
            l1_batch_commit_data_generator_mode: match rng.gen_range(0..2) {
                0 => L1BatchCommitmentMode::Rollup,
                _ => L1BatchCommitmentMode::Validium,
            },
        }
    }
}

impl Distribution<configs::EcosystemContracts> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::EcosystemContracts {
        configs::EcosystemContracts {
            bridgehub_proxy_addr: rng.gen(),
            state_transition_proxy_addr: rng.gen(),
            transparent_proxy_admin_addr: rng.gen(),
        }
    }
}

impl Distribution<configs::consensus::WeightedValidator> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::WeightedValidator {
        use configs::consensus::{ValidatorPublicKey, WeightedValidator};
        WeightedValidator {
            key: ValidatorPublicKey(self.sample(rng)),
            weight: self.sample(rng),
        }
    }
}

impl Distribution<configs::consensus::GenesisSpec> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::GenesisSpec {
        use configs::consensus::{GenesisSpec, ProtocolVersion, ValidatorPublicKey};
        GenesisSpec {
            chain_id: L2ChainId::default(),
            protocol_version: ProtocolVersion(self.sample(rng)),
            validators: self.sample_collect(rng),
            leader: ValidatorPublicKey(self.sample(rng)),
        }
    }
}

impl Distribution<configs::consensus::ConsensusConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::ConsensusConfig {
        use configs::consensus::{ConsensusConfig, Host, NodePublicKey};
        ConsensusConfig {
            server_addr: self.sample(rng),
            public_addr: Host(self.sample(rng)),
            max_payload_size: self.sample(rng),
            gossip_dynamic_inbound_limit: self.sample(rng),
            gossip_static_inbound: self
                .sample_range(rng)
                .map(|_| NodePublicKey(self.sample(rng)))
                .collect(),
            gossip_static_outbound: self
                .sample_range(rng)
                .map(|_| (NodePublicKey(self.sample(rng)), Host(self.sample(rng))))
                .collect(),
            genesis_spec: self.sample(rng),
            rpc: self.sample(rng),
        }
    }
}

impl Distribution<configs::consensus::RpcConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::RpcConfig {
        configs::consensus::RpcConfig {
            get_block_rate: self.sample(rng),
        }
    }
}

impl Distribution<configs::consensus::ConsensusSecrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::ConsensusSecrets {
        use configs::consensus::{ConsensusSecrets, NodeSecretKey, ValidatorSecretKey};
        ConsensusSecrets {
            validator_key: self.sample_opt(|| ValidatorSecretKey(String::into(self.sample(rng)))),
            node_key: self.sample_opt(|| NodeSecretKey(String::into(self.sample(rng)))),
        }
    }
}

impl Distribution<configs::secrets::L1Secrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::L1Secrets {
        use configs::secrets::L1Secrets;
        L1Secrets {
            l1_rpc_url: format!("localhost:{}", rng.gen::<u16>()).parse().unwrap(),
        }
    }
}

impl Distribution<configs::secrets::DatabaseSecrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::DatabaseSecrets {
        use configs::secrets::DatabaseSecrets;
        DatabaseSecrets {
            server_url: Some(format!("localhost:{}", rng.gen::<u16>()).parse().unwrap()),
            server_replica_url: Some(format!("localhost:{}", rng.gen::<u16>()).parse().unwrap()),
            prover_url: Some(format!("localhost:{}", rng.gen::<u16>()).parse().unwrap()),
        }
    }
}

impl Distribution<configs::secrets::Secrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::Secrets {
        use configs::secrets::Secrets;
        Secrets {
            consensus: self.sample_opt(|| self.sample(rng)),
            database: self.sample_opt(|| self.sample(rng)),
            l1: self.sample_opt(|| self.sample(rng)),
        }
    }
}

impl Distribution<configs::wallets::Wallet> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::Wallet {
        configs::wallets::Wallet::new(K256PrivateKey::from_bytes(rng.gen()).unwrap())
    }
}

impl Distribution<configs::wallets::AddressWallet> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::AddressWallet {
        configs::wallets::AddressWallet::from_address(rng.gen())
    }
}

impl Distribution<configs::wallets::StateKeeper> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::StateKeeper {
        configs::wallets::StateKeeper {
            fee_account: self.sample(rng),
        }
    }
}

impl Distribution<configs::wallets::EthSender> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::EthSender {
        configs::wallets::EthSender {
            operator: self.sample(rng),
            blob_operator: self.sample_opt(|| self.sample(rng)),
        }
    }
}

impl Distribution<configs::wallets::Wallets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::Wallets {
        configs::wallets::Wallets {
            state_keeper: self.sample_opt(|| self.sample(rng)),
            eth_sender: self.sample_opt(|| self.sample(rng)),
        }
    }
}

impl Distribution<configs::en_config::ENConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::en_config::ENConfig {
        configs::en_config::ENConfig {
            l2_chain_id: L2ChainId::default(),
            l1_chain_id: L1ChainId(rng.gen()),
            main_node_url: format!("localhost:{}", rng.gen::<u16>()).parse().unwrap(),
            l1_batch_commit_data_generator_mode: match rng.gen_range(0..2) {
                0 => L1BatchCommitmentMode::Rollup,
                _ => L1BatchCommitmentMode::Validium,
            },
            main_node_rate_limit_rps: self.sample_opt(|| rng.gen()),
        }
    }
}
