use std::num::NonZeroUsize;

use rand::{distributions::Distribution, Rng};
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    pubdata_da::PubdataSendingMode,
    secrets::{APIKey, SeedPhrase},
    vm::FastVmMode,
    L1BatchNumber, L1ChainId, L2ChainId, SLChainId,
};
use zksync_consensus_utils::EncodeDist;
use zksync_crypto_primitives::K256PrivateKey;

use crate::{
    configs::{
        self,
        api::DeploymentAllowlist,
        chain::TimestampAsserterConfig,
        da_client::{
            avail::{AvailClientConfig, AvailDefaultConfig},
            DAClientConfig::Avail,
        },
        external_price_api_client::ForcedPriceClientConfig,
    },
    AvailConfig,
};

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
            ws_port: self.sample(rng),
            req_entities_limit: self.sample(rng),
            filters_disabled: self.sample(rng),
            filters_limit: self.sample(rng),
            subscriptions_limit: self.sample(rng),
            pubsub_polling_interval: self.sample(rng),
            max_nonce_ahead: self.sample(rng),
            gas_price_scale_factor: self.sample(rng),
            estimate_gas_scale_factor: self.sample(rng),
            estimate_gas_acceptable_overestimation: self.sample(rng),
            estimate_gas_optimize_search: self.sample(rng),
            max_tx_size: self.sample(rng),
            vm_execution_cache_misses_limit: self.sample(rng),
            vm_concurrency_limit: self.sample(rng),
            factory_deps_cache_size_mb: self.sample(rng),
            initial_writes_cache_size_mb: self.sample(rng),
            latest_values_cache_size_mb: self.sample(rng),
            latest_values_max_block_lag: self.sample(rng),
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
            deployment_allowlist: DeploymentAllowlist::new(None, Some(300)),
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
            evm_emulator_hash: None,
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
            skip_unsafe_deposit_checks: self.sample(rng),
            l1_to_l2_txs_paused: self.sample(rng),
        }
    }
}

impl Distribution<configs::ContractVerifierConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ContractVerifierConfig {
        configs::ContractVerifierConfig {
            compilation_timeout: self.sample(rng),
            prometheus_port: self.sample(rng),
            port: self.sample(rng),
            etherscan_api_url: self.sample(rng),
        }
    }
}

impl Distribution<configs::AllContractsConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::AllContractsConfig {
        configs::AllContractsConfig {
            governance_addr: rng.gen(),
            verifier_addr: rng.gen(),
            default_upgrade_addr: rng.gen(),
            diamond_proxy_addr: rng.gen(),
            validator_timelock_addr: rng.gen(),
            l1_erc20_bridge_proxy_addr: self.sample_opt(|| rng.gen()),
            l2_erc20_bridge_addr: rng.gen(),
            l1_shared_bridge_proxy_addr: self.sample_opt(|| rng.gen()),
            l2_shared_bridge_addr: rng.gen(),
            l2_legacy_shared_bridge_addr: self.sample_opt(|| rng.gen()),
            l1_weth_bridge_proxy_addr: self.sample_opt(|| rng.gen()),
            l2_weth_bridge_addr: self.sample_opt(|| rng.gen()),
            l2_testnet_paymaster_addr: self.sample_opt(|| rng.gen()),
            l2_timestamp_asserter_addr: self.sample_opt(|| rng.gen()),
            l1_multicall3_addr: rng.gen(),
            bridgehub_proxy_addr: rng.gen(),
            state_transition_proxy_addr: self.sample_opt(|| rng.gen()),
            transparent_proxy_admin_addr: self.sample_opt(|| rng.gen()),
            l1_bytecode_supplier_addr: self.sample_opt(|| rng.gen()),
            l1_wrapped_base_token_store_addr: self.sample_opt(|| rng.gen()),
            base_token_addr: rng.gen(),
            l1_base_token_asset_id: self.sample_opt(|| rng.gen()),
            chain_admin_addr: rng.gen(),
            l2_da_validator_addr: self.sample_opt(|| rng.gen()),
            no_da_validium_l1_validator_addr: self.sample_opt(|| rng.gen()),
            l2_multicall3_addr: self.sample_opt(|| rng.gen()),
            server_notifier_addr: None,
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
            merkle_tree_repair_stale_keys: self.sample(rng),
        }
    }
}

impl Distribution<configs::ExperimentalVmPlaygroundConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ExperimentalVmPlaygroundConfig {
        configs::ExperimentalVmPlaygroundConfig {
            fast_vm_mode: gen_fast_vm_mode(rng),
            db_path: self.sample(rng),
            first_processed_batch: L1BatchNumber(rng.gen()),
            window_size: rng.gen(),
            reset: self.sample(rng),
        }
    }
}

fn gen_fast_vm_mode<R: Rng + ?Sized>(rng: &mut R) -> FastVmMode {
    match rng.gen_range(0..3) {
        0 => FastVmMode::Old,
        1 => FastVmMode::New,
        _ => FastVmMode::Shadow,
    }
}

impl Distribution<configs::ExperimentalVmConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::ExperimentalVmConfig {
        configs::ExperimentalVmConfig {
            playground: self.sample(rng),
            state_keeper_fast_vm_mode: gen_fast_vm_mode(rng),
            api_fast_vm_mode: gen_fast_vm_mode(rng),
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
        }
    }
}

impl Distribution<configs::EthConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::EthConfig {
        configs::EthConfig::new(self.sample(rng), self.sample(rng), self.sample(rng))
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

impl Distribution<configs::eth_sender::GasLimitMode> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::eth_sender::GasLimitMode {
        type T = configs::eth_sender::GasLimitMode;
        match rng.gen_range(0..2) {
            0 => T::Maximum,
            _ => T::Calculated,
        }
    }
}

impl Distribution<configs::eth_sender::SenderConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::eth_sender::SenderConfig {
        configs::eth_sender::SenderConfig {
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
            tx_aggregation_paused: false,
            tx_aggregation_only_prove_and_execute: false,
            time_in_mempool_in_l1_blocks_cap: self.sample(rng),
            is_verifier_pre_fflonk: self.sample(rng),
            gas_limit_mode: self.sample(rng),
            max_acceptable_base_fee_in_wei: self.sample(rng),
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

impl Distribution<configs::FriProverConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::FriProverConfig {
        configs::FriProverConfig {
            setup_data_path: self.sample(rng),
            prometheus_port: self.sample(rng),
            max_attempts: self.sample(rng),
            generation_timeout_in_secs: self.sample(rng),
            prover_object_store: self.sample(rng),
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
            api_mode: self.sample(rng),
            port: self.sample(rng),
        }
    }
}

impl Distribution<configs::fri_prover_gateway::ApiMode> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::fri_prover_gateway::ApiMode {
        type T = configs::fri_prover_gateway::ApiMode;

        match rng.gen_range(0..2) {
            0 => T::Legacy,
            _ => T::ProverCluster,
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
            prometheus_listener_port: self.sample(rng),
            max_circuits_in_flight: self.sample(rng),
        }
    }
}

impl Distribution<configs::house_keeper::HouseKeeperConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::house_keeper::HouseKeeperConfig {
        configs::house_keeper::HouseKeeperConfig {
            l1_batch_metrics_reporting_interval_ms: self.sample(rng),
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
            gateway_api_url: self.sample(rng),
            proof_fetch_interval_in_secs: self.sample(rng),
            proof_gen_data_submit_interval_in_secs: self.sample(rng),
            fetch_zero_chain_id_proofs: self.sample(rng),
        }
    }
}

impl Distribution<configs::TeeProofDataHandlerConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::TeeProofDataHandlerConfig {
        configs::TeeProofDataHandlerConfig {
            http_port: self.sample(rng),
            first_processed_batch: L1BatchNumber(rng.gen()),
            proof_generation_timeout_in_secs: self.sample(rng),
            batch_permanently_ignored_timeout_in_hours: self.sample(rng),
            dcap_collateral_refresh_in_secs: self.sample(rng),
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
            logs_endpoint: self.sample(rng),
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
            evm_emulator_hash: Some(rng.gen()),
            fee_account: rng.gen(),
            l1_chain_id: L1ChainId(self.sample(rng)),
            l2_chain_id: L2ChainId::default(),
            snark_wrapper_vk_hash: rng.gen(),
            fflonk_snark_wrapper_vk_hash: Some(rng.gen()),
            dummy_verifier: rng.gen(),
            l1_batch_commit_data_generator_mode: match rng.gen_range(0..2) {
                0 => L1BatchCommitmentMode::Rollup,
                _ => L1BatchCommitmentMode::Validium,
            },
            custom_genesis_state_path: None,
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

impl Distribution<configs::consensus::WeightedAttester> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::WeightedAttester {
        use configs::consensus::{AttesterPublicKey, WeightedAttester};
        WeightedAttester {
            key: AttesterPublicKey(self.sample(rng)),
            weight: self.sample(rng),
        }
    }
}

impl Distribution<configs::consensus::GenesisSpec> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::GenesisSpec {
        use configs::consensus::{
            GenesisSpec, Host, NodePublicKey, ProtocolVersion, ValidatorPublicKey,
        };
        GenesisSpec {
            chain_id: L2ChainId::default(),
            protocol_version: ProtocolVersion(self.sample(rng)),
            validators: self.sample_collect(rng),
            attesters: self.sample_collect(rng),
            leader: ValidatorPublicKey(self.sample(rng)),
            registry_address: self.sample_opt(|| rng.gen()),
            seed_peers: self
                .sample_range(rng)
                .map(|_| (NodePublicKey(self.sample(rng)), Host(self.sample(rng))))
                .collect(),
        }
    }
}

impl Distribution<configs::consensus::ConsensusConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::consensus::ConsensusConfig {
        use configs::consensus::{ConsensusConfig, Host, NodePublicKey};
        ConsensusConfig {
            port: self.sample(rng),
            server_addr: self.sample(rng),
            public_addr: Host(self.sample(rng)),
            max_payload_size: self.sample(rng),
            view_timeout: self.sample(rng),
            max_batch_size: self.sample(rng),
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
            debug_page_addr: self.sample(rng),
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
        use configs::consensus::{
            AttesterSecretKey, ConsensusSecrets, NodeSecretKey, ValidatorSecretKey,
        };
        ConsensusSecrets {
            validator_key: self.sample_opt(|| ValidatorSecretKey(String::into(self.sample(rng)))),
            attester_key: self.sample_opt(|| AttesterSecretKey(String::into(self.sample(rng)))),
            node_key: self.sample_opt(|| NodeSecretKey(String::into(self.sample(rng)))),
        }
    }
}

impl Distribution<configs::secrets::L1Secrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::L1Secrets {
        use configs::secrets::L1Secrets;
        L1Secrets {
            l1_rpc_url: format!("localhost:{}", rng.gen::<u16>()).parse().unwrap(),
            gateway_rpc_url: Some(format!("localhost:{}", rng.gen::<u16>()).parse().unwrap()),
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
            data_availability: self.sample_opt(|| self.sample(rng)),
            contract_verifier: self.sample_opt(|| self.sample(rng)),
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

impl Distribution<configs::wallets::TokenMultiplierSetter> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::TokenMultiplierSetter {
        configs::wallets::TokenMultiplierSetter {
            wallet: self.sample(rng),
        }
    }
}

impl Distribution<configs::wallets::Wallets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::wallets::Wallets {
        configs::wallets::Wallets {
            state_keeper: self.sample_opt(|| self.sample(rng)),
            eth_sender: self.sample_opt(|| self.sample(rng)),
            token_multiplier_setter: self.sample_opt(|| self.sample(rng)),
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
            bridge_addresses_refresh_interval_sec: self.sample_opt(|| rng.gen()),
            gateway_chain_id: self.sample_opt(|| SLChainId(rng.gen())),
        }
    }
}

impl Distribution<configs::da_client::DAClientConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::da_client::DAClientConfig {
        Avail(AvailConfig {
            bridge_api_url: self.sample(rng),
            timeout_ms: self.sample(rng),
            config: AvailClientConfig::FullClient(AvailDefaultConfig {
                api_node_url: self.sample(rng),
                app_id: self.sample(rng),
                finality_state: None,
                dispatch_timeout_ms: self.sample(rng),
            }),
        })
    }
}

impl Distribution<configs::secrets::DataAvailabilitySecrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::DataAvailabilitySecrets {
        configs::secrets::DataAvailabilitySecrets::Avail(configs::da_client::avail::AvailSecrets {
            seed_phrase: Some(<SeedPhrase as From<String>>::from(self.sample(rng))),
            gas_relay_api_key: Some(<APIKey as From<String>>::from(self.sample(rng))),
        })
    }
}

impl Distribution<configs::da_dispatcher::DADispatcherConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::da_dispatcher::DADispatcherConfig {
        configs::da_dispatcher::DADispatcherConfig {
            polling_interval_ms: self.sample(rng),
            max_rows_to_dispatch: self.sample(rng),
            max_retries: self.sample(rng),
            use_dummy_inclusion_data: self.sample(rng),
            inclusion_verification_transition_enabled: self.sample(rng),
        }
    }
}

impl Distribution<configs::vm_runner::ProtectiveReadsWriterConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::vm_runner::ProtectiveReadsWriterConfig {
        configs::vm_runner::ProtectiveReadsWriterConfig {
            db_path: self.sample(rng),
            window_size: self.sample(rng),
            first_processed_batch: L1BatchNumber(rng.gen()),
        }
    }
}

impl Distribution<configs::vm_runner::BasicWitnessInputProducerConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::vm_runner::BasicWitnessInputProducerConfig {
        configs::vm_runner::BasicWitnessInputProducerConfig {
            db_path: self.sample(rng),
            window_size: self.sample(rng),
            first_processed_batch: L1BatchNumber(rng.gen()),
        }
    }
}

impl Distribution<configs::CommitmentGeneratorConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::CommitmentGeneratorConfig {
        configs::CommitmentGeneratorConfig {
            max_parallelism: self.sample(rng),
        }
    }
}

impl Distribution<configs::snapshot_recovery::TreeRecoveryConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::snapshot_recovery::TreeRecoveryConfig {
        configs::snapshot_recovery::TreeRecoveryConfig {
            chunk_size: self.sample(rng),
            parallel_persistence_buffer: self.sample_opt(|| rng.gen()),
        }
    }
}

impl Distribution<configs::snapshot_recovery::PostgresRecoveryConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::snapshot_recovery::PostgresRecoveryConfig {
        configs::snapshot_recovery::PostgresRecoveryConfig {
            max_concurrency: self.sample_opt(|| rng.gen()),
        }
    }
}

impl Distribution<configs::snapshot_recovery::SnapshotRecoveryConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::snapshot_recovery::SnapshotRecoveryConfig {
        use configs::snapshot_recovery::{SnapshotRecoveryConfig, TreeRecoveryConfig};
        let tree: TreeRecoveryConfig = self.sample(rng);
        SnapshotRecoveryConfig {
            enabled: self.sample(rng),
            l1_batch: self.sample_opt(|| L1BatchNumber(rng.gen())),
            drop_storage_key_preimages: (tree != TreeRecoveryConfig::default()) && self.sample(rng),
            tree,
            postgres: self.sample(rng),
            object_store: self.sample(rng),
        }
    }
}

impl Distribution<configs::pruning::PruningConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::pruning::PruningConfig {
        configs::pruning::PruningConfig {
            enabled: self.sample(rng),
            chunk_size: self.sample(rng),
            removal_delay_sec: self.sample_opt(|| rng.gen()),
            data_retention_sec: self.sample(rng),
        }
    }
}

impl Distribution<configs::base_token_adjuster::BaseTokenAdjusterConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::base_token_adjuster::BaseTokenAdjusterConfig {
        configs::base_token_adjuster::BaseTokenAdjusterConfig {
            price_polling_interval_ms: self.sample(rng),
            price_cache_update_interval_ms: self.sample(rng),
            max_tx_gas: self.sample(rng),
            default_priority_fee_per_gas: self.sample(rng),
            max_acceptable_priority_fee_in_gwei: self.sample(rng),
            l1_receipt_checking_max_attempts: self.sample(rng),
            l1_receipt_checking_sleep_ms: self.sample(rng),
            l1_tx_sending_max_attempts: self.sample(rng),
            l1_tx_sending_sleep_ms: self.sample(rng),
            l1_update_deviation_percentage: self.sample(rng),
            price_fetching_max_attempts: self.sample(rng),
            price_fetching_sleep_ms: self.sample(rng),
            halt_on_error: self.sample(rng),
        }
    }
}

impl Distribution<configs::external_proof_integration_api::ExternalProofIntegrationApiConfig>
    for EncodeDist
{
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::external_proof_integration_api::ExternalProofIntegrationApiConfig {
        configs::external_proof_integration_api::ExternalProofIntegrationApiConfig {
            http_port: self.sample(rng),
        }
    }
}

impl Distribution<configs::external_price_api_client::ExternalPriceApiClientConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::external_price_api_client::ExternalPriceApiClientConfig {
        configs::external_price_api_client::ExternalPriceApiClientConfig {
            source: self.sample(rng),
            base_url: self.sample(rng),
            api_key: self.sample(rng),
            client_timeout_ms: self.sample(rng),
            forced: Some(ForcedPriceClientConfig {
                numerator: self.sample(rng),
                denominator: self.sample(rng),
                fluctuation: self.sample(rng),
                next_value_fluctuation: self.sample(rng),
            }),
        }
    }
}

impl Distribution<configs::prover_job_monitor::ProverJobMonitorConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> configs::prover_job_monitor::ProverJobMonitorConfig {
        configs::prover_job_monitor::ProverJobMonitorConfig {
            prometheus_port: self.sample(rng),
            max_db_connections: self.sample(rng),
            graceful_shutdown_timeout_ms: self.sample(rng),
            gpu_prover_archiver_run_interval_ms: self.sample(rng),
            gpu_prover_archiver_archive_prover_after_ms: self.sample(rng),
            prover_jobs_archiver_run_interval_ms: self.sample(rng),
            prover_jobs_archiver_archive_jobs_after_ms: self.sample(rng),
            proof_compressor_job_requeuer_run_interval_ms: self.sample(rng),
            prover_job_requeuer_run_interval_ms: self.sample(rng),
            witness_generator_job_requeuer_run_interval_ms: self.sample(rng),
            proof_compressor_queue_reporter_run_interval_ms: self.sample(rng),
            prover_queue_reporter_run_interval_ms: self.sample(rng),
            witness_generator_queue_reporter_run_interval_ms: self.sample(rng),
            witness_job_queuer_run_interval_ms: self.sample(rng),
            http_port: self.sample(rng),
        }
    }
}

impl Distribution<configs::GeneralConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::GeneralConfig {
        configs::GeneralConfig {
            postgres_config: self.sample(rng),
            api_config: self.sample(rng),
            contract_verifier: self.sample(rng),
            circuit_breaker_config: self.sample(rng),
            mempool_config: self.sample(rng),
            operations_manager_config: self.sample(rng),
            state_keeper_config: self.sample(rng),
            house_keeper_config: self.sample(rng),
            proof_compressor_config: self.sample(rng),
            prover_config: self.sample(rng),
            prover_gateway: self.sample(rng),
            witness_generator_config: self.sample(rng),
            prometheus_config: self.sample(rng),
            proof_data_handler_config: self.sample(rng),
            tee_proof_data_handler_config: self.sample(rng),
            db_config: self.sample(rng),
            eth: self.sample(rng),
            snapshot_creator: self.sample(rng),
            observability: self.sample(rng),
            da_client_config: self.sample(rng),
            da_dispatcher_config: self.sample(rng),
            protective_reads_writer_config: self.sample(rng),
            basic_witness_input_producer_config: self.sample(rng),
            commitment_generator: self.sample(rng),
            snapshot_recovery: self.sample(rng),
            pruning: self.sample(rng),
            core_object_store: self.sample(rng),
            base_token_adjuster: self.sample(rng),
            external_price_api_client_config: self.sample(rng),
            consensus_config: self.sample(rng),
            external_proof_integration_api_config: self.sample(rng),
            experimental_vm_config: self.sample(rng),
            prover_job_monitor_config: self.sample(rng),
            timestamp_asserter_config: self.sample(rng),
        }
    }
}

impl Distribution<TimestampAsserterConfig> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> TimestampAsserterConfig {
        TimestampAsserterConfig {
            min_time_till_end_sec: self.sample(rng),
        }
    }
}

impl Distribution<configs::secrets::ContractVerifierSecrets> for EncodeDist {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> configs::secrets::ContractVerifierSecrets {
        configs::secrets::ContractVerifierSecrets {
            etherscan_api_key: Some(<APIKey as From<String>>::from(self.sample(rng))),
        }
    }
}
