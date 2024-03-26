use std::collections::HashSet;

use rand::{distributions::Alphanumeric, Rng};
use zksync_basic_types::{
    basic_fri_types::CircuitIdRoundTuple, network::Network, Address, L1ChainId, L2ChainId, H256,
};

use crate::configs::{self, eth_sender::PubdataSendingMode};

/// Generator of random configs.
pub struct Gen<'a, R: Rng> {
    /// Underlying RNG.
    pub rng: &'a mut R,
    /// Generate configs with only required fields.
    pub required_only: bool,
    /// Generate decimal fractions for f64
    /// to avoid rounding errors of decimal encodings.
    pub decimal_fractions: bool,
}

impl<'a, R: Rng> Gen<'a, R> {
    pub fn gen<C: RandomConfig>(&mut self) -> C {
        C::sample(self)
    }
}

pub trait RandomConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self;
}

impl RandomConfig for std::net::SocketAddr {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        std::net::SocketAddr::new(std::net::IpAddr::from(g.rng.gen::<[u8; 16]>()), g.gen())
    }
}

impl RandomConfig for String {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        let n = g.rng.gen_range(5..10);
        g.rng
            .sample_iter(&Alphanumeric)
            .take(n)
            .map(char::from)
            .collect()
    }
}

impl<T: RandomConfig> RandomConfig for Option<T> {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        if g.required_only {
            return None;
        }
        Some(g.gen())
    }
}

impl<T: RandomConfig> RandomConfig for Vec<T> {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        if g.required_only {
            return vec![];
        }
        (0..g.rng.gen_range(5..10)).map(|_| g.gen()).collect()
    }
}

impl<T: RandomConfig + Eq + std::hash::Hash> RandomConfig for HashSet<T> {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        if g.required_only {
            return Self::new();
        }
        (0..g.rng.gen_range(5..10)).map(|_| g.gen()).collect()
    }
}

impl RandomConfig for bool {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        g.rng.gen()
    }
}

impl RandomConfig for u8 {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        g.rng.gen()
    }
}

impl RandomConfig for u16 {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        g.rng.gen()
    }
}

impl RandomConfig for u32 {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        g.rng.gen()
    }
}

impl RandomConfig for u64 {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        g.rng.gen()
    }
}

impl RandomConfig for f64 {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        if g.decimal_fractions {
            const PRECISION: usize = 1000000;
            return g.rng.gen_range(0..PRECISION) as f64 / PRECISION as f64;
        }
        g.rng.gen()
    }
}

impl RandomConfig for usize {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        g.rng.gen()
    }
}

impl RandomConfig for std::num::NonZeroU32 {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        g.rng.gen()
    }
}

impl RandomConfig for Address {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        g.rng.gen()
    }
}

impl RandomConfig for H256 {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        g.rng.gen()
    }
}

impl RandomConfig for Network {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..8) {
            0 => Self::Mainnet,
            1 => Self::Rinkeby,
            2 => Self::Ropsten,
            3 => Self::Goerli,
            4 => Self::Sepolia,
            5 => Self::Localhost,
            6 => Self::Unknown,
            _ => Self::Test,
        }
    }
}

impl RandomConfig for configs::chain::FeeModelVersion {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..2) {
            0 => Self::V1,
            _ => Self::V2,
        }
    }
}

impl RandomConfig for configs::AlertsConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            sporadic_crypto_errors_substrs: g.gen(),
        }
    }
}

impl RandomConfig for configs::ApiConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            web3_json_rpc: g.gen(),
            contract_verification: g.gen(),
            prometheus: g.gen(),
            healthcheck: g.gen(),
            merkle_tree: g.gen(),
        }
    }
}

impl RandomConfig for configs::api::Web3JsonRpcConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            http_port: g.gen(),
            http_url: g.gen(),
            ws_port: g.gen(),
            ws_url: g.gen(),
            req_entities_limit: g.gen(),
            filters_disabled: g.gen(),
            filters_limit: g.gen(),
            subscriptions_limit: g.gen(),
            pubsub_polling_interval: g.gen(),
            max_nonce_ahead: g.gen(),
            gas_price_scale_factor: g.gen(),
            request_timeout: g.gen(),
            account_pks: g.gen(),
            estimate_gas_scale_factor: g.gen(),
            estimate_gas_acceptable_overestimation: g.gen(),
            l1_to_l2_transactions_compatibility_mode: g.gen(),
            max_tx_size: g.gen(),
            vm_execution_cache_misses_limit: g.gen(),
            vm_concurrency_limit: g.gen(),
            factory_deps_cache_size_mb: g.gen(),
            initial_writes_cache_size_mb: g.gen(),
            latest_values_cache_size_mb: g.gen(),
            fee_history_limit: g.gen(),
            max_batch_request_size: g.gen(),
            max_response_body_size_mb: g.gen(),
            websocket_requests_per_minute_limit: g.gen(),
            tree_api_url: g.gen(),
            mempool_cache_update_interval: g.gen(),
            mempool_cache_size: g.gen(),
        }
    }
}

impl RandomConfig for configs::api::HealthCheckConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            port: g.gen(),
            slow_time_limit_ms: g.gen(),
            hard_time_limit_ms: g.gen(),
        }
    }
}

impl RandomConfig for configs::api::ContractVerificationApiConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            port: g.gen(),
            url: g.gen(),
        }
    }
}

impl RandomConfig for configs::api::MerkleTreeApiConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self { port: g.gen() }
    }
}

impl RandomConfig for configs::PrometheusConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            listener_port: g.gen(),
            pushgateway_url: g.gen(),
            push_interval_ms: g.gen(),
        }
    }
}

impl RandomConfig for configs::chain::NetworkConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            network: g.gen(),
            zksync_network: g.gen(),
            zksync_network_id: L2ChainId::max(),
        }
    }
}

impl RandomConfig for configs::chain::StateKeeperConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            transaction_slots: g.gen(),
            block_commit_deadline_ms: g.gen(),
            miniblock_commit_deadline_ms: g.gen(),
            miniblock_seal_queue_capacity: g.gen(),
            max_single_tx_gas: g.gen(),
            max_allowed_l2_tx_gas_limit: g.gen(),
            reject_tx_at_geometry_percentage: g.gen(),
            reject_tx_at_eth_params_percentage: g.gen(),
            reject_tx_at_gas_percentage: g.gen(),
            close_block_at_geometry_percentage: g.gen(),
            close_block_at_eth_params_percentage: g.gen(),
            close_block_at_gas_percentage: g.gen(),
            fee_account_addr: g.gen(),
            minimal_l2_gas_price: g.gen(),
            compute_overhead_part: g.gen(),
            pubdata_overhead_part: g.gen(),
            batch_overhead_l1_gas: g.gen(),
            max_gas_per_batch: g.gen(),
            max_pubdata_per_batch: g.gen(),
            fee_model_version: g.gen(),
            validation_computational_gas_limit: g.gen(),
            save_call_traces: g.gen(),
            virtual_blocks_interval: g.gen(),
            virtual_blocks_per_miniblock: g.gen(),
            upload_witness_inputs_to_gcs: g.gen(),
            enum_index_migration_chunk_size: g.gen(),
            bootloader_hash: g.gen(),
            default_aa_hash: g.gen(),
        }
    }
}

impl RandomConfig for configs::chain::OperationsManagerConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            delay_interval: g.gen(),
        }
    }
}

impl RandomConfig for configs::chain::CircuitBreakerConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            sync_interval_ms: g.gen(),
            http_req_max_retry_number: g.gen(),
            http_req_retry_interval_sec: g.gen(),
            replication_lag_limit_sec: g.gen(),
        }
    }
}

impl RandomConfig for configs::chain::MempoolConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            sync_interval_ms: g.gen(),
            sync_batch_size: g.gen(),
            capacity: g.gen(),
            stuck_tx_timeout: g.gen(),
            remove_stuck_txs: g.gen(),
            delay_interval: g.gen(),
        }
    }
}

impl RandomConfig for configs::ContractVerifierConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            compilation_timeout: g.gen(),
            polling_interval: g.gen(),
            prometheus_port: g.gen(),
        }
    }
}

impl RandomConfig for configs::ContractsConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            governance_addr: g.gen(),
            mailbox_facet_addr: g.gen(),
            executor_facet_addr: g.gen(),
            admin_facet_addr: g.gen(),
            getters_facet_addr: g.gen(),
            verifier_addr: g.gen(),
            diamond_init_addr: g.gen(),
            diamond_upgrade_init_addr: g.gen(),
            diamond_proxy_addr: g.gen(),
            validator_timelock_addr: g.gen(),
            genesis_tx_hash: g.gen(),
            l1_erc20_bridge_proxy_addr: g.gen(),
            l1_erc20_bridge_impl_addr: g.gen(),
            l2_erc20_bridge_addr: g.gen(),
            l1_weth_bridge_proxy_addr: g.gen(),
            l2_weth_bridge_addr: g.gen(),
            l1_allow_list_addr: g.gen(),
            l2_testnet_paymaster_addr: g.gen(),
            recursion_scheduler_level_vk_hash: g.gen(),
            recursion_node_level_vk_hash: g.gen(),
            recursion_leaf_level_vk_hash: g.gen(),
            recursion_circuits_set_vks_hash: g.gen(),
            l1_multicall3_addr: g.gen(),
            fri_recursion_scheduler_level_vk_hash: g.gen(),
            fri_recursion_node_level_vk_hash: g.gen(),
            fri_recursion_leaf_level_vk_hash: g.gen(),
            snark_wrapper_vk_hash: g.gen(),
            bridgehub_impl_addr: g.gen(),
            bridgehub_proxy_addr: g.gen(),
            state_transition_proxy_addr: g.gen(),
            state_transition_impl_addr: g.gen(),
            transparent_proxy_admin_addr: g.gen(),
            genesis_batch_commitment: g.gen(),
            genesis_rollup_leaf_index: g.gen(),
            genesis_root: g.gen(),
            genesis_protocol_version: g.gen(),
        }
    }
}

impl RandomConfig for configs::database::MerkleTreeMode {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..2) {
            0 => Self::Full,
            _ => Self::Lightweight,
        }
    }
}

impl RandomConfig for configs::database::MerkleTreeConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            path: g.gen(),
            mode: g.gen(),
            multi_get_chunk_size: g.gen(),
            block_cache_size_mb: g.gen(),
            memtable_capacity_mb: g.gen(),
            stalled_writes_timeout_sec: g.gen(),
            max_l1_batches_per_iter: g.gen(),
        }
    }
}

impl RandomConfig for configs::database::DBConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            state_keeper_db_path: g.gen(),
            merkle_tree: g.gen(),
        }
    }
}

impl RandomConfig for configs::database::PostgresConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            master_url: g.gen(),
            replica_url: g.gen(),
            prover_url: g.gen(),
            max_connections: g.gen(),
            max_connections_master: g.gen(),
            acquire_timeout_sec: g.gen(),
            statement_timeout_sec: g.gen(),
            long_connection_threshold_ms: g.gen(),
            slow_query_threshold_ms: g.gen(),
        }
    }
}

impl RandomConfig for configs::ETHClientConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            chain_id: g.gen(),
            web3_url: g.gen(),
        }
    }
}

impl RandomConfig for configs::ETHSenderConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            sender: g.gen(),
            gas_adjuster: g.gen(),
        }
    }
}

impl RandomConfig for configs::eth_sender::ProofSendingMode {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..3) {
            0 => Self::OnlyRealProofs,
            1 => Self::OnlySampledProofs,
            _ => Self::SkipEveryProof,
        }
    }
}

impl RandomConfig for configs::eth_sender::ProofLoadingMode {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..2) {
            0 => Self::OldProofFromDb,
            _ => Self::FriProofFromGcs,
        }
    }
}

impl RandomConfig for configs::eth_sender::PubdataSendingMode {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..2) {
            0 => Self::Calldata,
            _ => Self::Blobs,
        }
    }
}

impl RandomConfig for configs::eth_sender::SenderConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            aggregated_proof_sizes: g.gen(),
            wait_confirmations: g.gen(),
            tx_poll_period: g.gen(),
            aggregate_tx_poll_period: g.gen(),
            max_txs_in_flight: g.gen(),
            proof_sending_mode: g.gen(),
            max_aggregated_tx_gas: g.gen(),
            max_eth_tx_data_size: g.gen(),
            max_aggregated_blocks_to_commit: g.gen(),
            max_aggregated_blocks_to_execute: g.gen(),
            aggregated_block_commit_deadline: g.gen(),
            aggregated_block_prove_deadline: g.gen(),
            aggregated_block_execute_deadline: g.gen(),
            timestamp_criteria_max_allowed_lag: g.gen(),
            l1_batch_min_age_before_execute_seconds: g.gen(),
            max_acceptable_priority_fee_in_gwei: g.gen(),
            proof_loading_mode: g.gen(),
            pubdata_sending_mode: PubdataSendingMode::Calldata,
        }
    }
}

impl RandomConfig for configs::eth_sender::GasAdjusterConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            default_priority_fee_per_gas: g.gen(),
            max_base_fee_samples: g.gen(),
            pricing_formula_parameter_a: g.gen(),
            pricing_formula_parameter_b: g.gen(),
            internal_l1_pricing_multiplier: g.gen(),
            internal_enforced_l1_gas_price: g.gen(),
            poll_period: g.gen(),
            max_l1_gas_price: g.gen(),
            num_samples_for_blob_base_fee_estimate: g.gen(),
            internal_pubdata_pricing_multiplier: g.gen(),
            max_blob_base_fee: g.gen(),
        }
    }
}

impl RandomConfig for configs::ETHWatchConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            confirmations_for_eth_event: g.gen(),
            eth_node_poll_interval: g.gen(),
        }
    }
}

impl RandomConfig for configs::FriProofCompressorConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            compression_mode: g.gen(),
            prometheus_listener_port: g.gen(),
            prometheus_pushgateway_url: g.gen(),
            prometheus_push_interval_ms: g.gen(),
            generation_timeout_in_secs: g.gen(),
            max_attempts: g.gen(),
            universal_setup_path: g.gen(),
            universal_setup_download_url: g.gen(),
            verify_wrapper_proof: g.gen(),
        }
    }
}

impl RandomConfig for configs::fri_prover::SetupLoadMode {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..2) {
            0 => Self::FromDisk,
            _ => Self::FromMemory,
        }
    }
}

impl RandomConfig for configs::FriProverConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            setup_data_path: g.gen(),
            prometheus_port: g.gen(),
            max_attempts: g.gen(),
            generation_timeout_in_secs: g.gen(),
            base_layer_circuit_ids_to_be_verified: g.gen(),
            recursive_layer_circuit_ids_to_be_verified: g.gen(),
            setup_load_mode: g.gen(),
            specialized_group_id: g.gen(),
            witness_vector_generator_thread_count: g.gen(),
            queue_capacity: g.gen(),
            witness_vector_receiver_port: g.gen(),
            zone_read_url: g.gen(),
            shall_save_to_public_bucket: g.gen(),
        }
    }
}

impl RandomConfig for configs::FriProverGatewayConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            api_url: g.gen(),
            api_poll_duration_secs: g.gen(),
            prometheus_listener_port: g.gen(),
            prometheus_pushgateway_url: g.gen(),
            prometheus_push_interval_ms: g.gen(),
        }
    }
}

impl RandomConfig for CircuitIdRoundTuple {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            circuit_id: g.gen(),
            aggregation_round: g.gen(),
        }
    }
}

impl RandomConfig for configs::fri_prover_group::FriProverGroupConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            group_0: g.gen(),
            group_1: g.gen(),
            group_2: g.gen(),
            group_3: g.gen(),
            group_4: g.gen(),
            group_5: g.gen(),
            group_6: g.gen(),
            group_7: g.gen(),
            group_8: g.gen(),
            group_9: g.gen(),
            group_10: g.gen(),
            group_11: g.gen(),
            group_12: g.gen(),
        }
    }
}

impl RandomConfig for configs::FriWitnessGeneratorConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            generation_timeout_in_secs: g.gen(),
            max_attempts: g.gen(),
            blocks_proving_percentage: g.gen(),
            dump_arguments_for_blocks: g.gen(),
            last_l1_batch_to_process: g.gen(),
            force_process_block: g.gen(),
            shall_save_to_public_bucket: g.gen(),
        }
    }
}

impl RandomConfig for configs::FriWitnessVectorGeneratorConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            max_prover_reservation_duration_in_secs: g.gen(),
            prover_instance_wait_timeout_in_secs: g.gen(),
            prover_instance_poll_time_in_milli_secs: g.gen(),
            prometheus_listener_port: g.gen(),
            prometheus_pushgateway_url: g.gen(),
            prometheus_push_interval_ms: g.gen(),
            specialized_group_id: g.gen(),
        }
    }
}

impl RandomConfig for configs::house_keeper::HouseKeeperConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            l1_batch_metrics_reporting_interval_ms: g.gen(),
            gpu_prover_queue_reporting_interval_ms: g.gen(),
            prover_job_retrying_interval_ms: g.gen(),
            prover_stats_reporting_interval_ms: g.gen(),
            witness_job_moving_interval_ms: g.gen(),
            witness_generator_stats_reporting_interval_ms: g.gen(),
            fri_witness_job_moving_interval_ms: g.gen(),
            fri_prover_job_retrying_interval_ms: g.gen(),
            fri_witness_generator_job_retrying_interval_ms: g.gen(),
            prover_db_pool_size: g.gen(),
            fri_prover_stats_reporting_interval_ms: g.gen(),
            fri_proof_compressor_job_retrying_interval_ms: g.gen(),
            fri_proof_compressor_stats_reporting_interval_ms: g.gen(),
        }
    }
}

impl RandomConfig for configs::object_store::ObjectStoreMode {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..4) {
            0 => Self::GCS {
                bucket_base_url: g.gen(),
            },
            1 => Self::GCSWithCredentialFile {
                bucket_base_url: g.gen(),
                gcs_credential_file_path: g.gen(),
            },
            2 => Self::FileBacked {
                file_backed_base_path: g.gen(),
            },
            _ => Self::GCSAnonymousReadOnly {
                bucket_base_url: g.gen(),
            },
        }
    }
}

impl RandomConfig for configs::ObjectStoreConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            mode: g.gen(),
            max_retries: g.gen(),
        }
    }
}

impl RandomConfig for configs::proof_data_handler::ProtocolVersionLoadingMode {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..2) {
            0 => Self::FromDb,
            _ => Self::FromEnvVar,
        }
    }
}

impl RandomConfig for configs::ProofDataHandlerConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            http_port: g.gen(),
            proof_generation_timeout_in_secs: g.gen(),
            protocol_version_loading_mode: g.gen(),
            fri_protocol_version_id: g.gen(),
        }
    }
}

impl RandomConfig for configs::SnapshotsCreatorConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            storage_logs_chunk_size: g.gen(),
            concurrent_queries_count: g.gen(),
        }
    }
}

impl RandomConfig for configs::witness_generator::BasicWitnessGeneratorDataSource {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        match g.rng.gen_range(0..2) {
            0 => Self::FromPostgres,
            1 => Self::FromPostgresShadowBlob,
            _ => Self::FromBlob,
        }
    }
}

impl RandomConfig for configs::WitnessGeneratorConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            generation_timeout_in_secs: g.gen(),
            initial_setup_key_path: g.gen(),
            key_download_url: g.gen(),
            max_attempts: g.gen(),
            blocks_proving_percentage: g.gen(),
            dump_arguments_for_blocks: g.gen(),
            last_l1_batch_to_process: g.gen(),
            data_source: g.gen(),
        }
    }
}

impl RandomConfig for configs::ObservabilityConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            sentry_url: g.gen(),
            sentry_environment: g.gen(),
            log_format: g.gen(),
            opentelemetry: g.gen(),
        }
    }
}

impl RandomConfig for configs::OpentelemetryConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            level: g.gen(),
            endpoint: g.gen(),
        }
    }
}

impl RandomConfig for configs::GenesisConfig {
    fn sample(g: &mut Gen<impl Rng>) -> Self {
        Self {
            protocol_version: g.gen(),
            genesis_root_hash: g.gen(),
            rollup_last_leaf_index: g.gen(),
            genesis_commitment: g.gen(),
            bootloader_hash: g.gen(),
            default_aa_hash: g.gen(),
            fee_account: g.gen(),
            l1_chain_id: L1ChainId(g.gen()),
            l2_chain_id: L2ChainId::default(),
            recursion_node_level_vk_hash: g.gen(),
            recursion_leaf_level_vk_hash: g.gen(),
            recursion_scheduler_level_vk_hash: g.gen(),
        }
    }
}
