//! Tests for EN configuration.

use std::{
    num::{NonZeroU32, NonZeroUsize},
    time::Duration,
};

use assert_matches::assert_matches;
use smart_config::{testing::Tester, value::ExposeSecret, ByteSize, ConfigSource, Yaml};
use zksync_config::configs::{
    api::{HealthCheckConfig, MaxResponseSizeOverrides},
    chain::TimestampAsserterConfig,
    da_client::{
        avail::{AvailClientConfig, AvailFinalityState},
        eigen::PointsSource,
    },
    database::MerkleTreeMode,
    object_store::ObjectStoreMode,
    CommitmentGeneratorConfig,
};
use zksync_types::{L1BatchNumber, L1ChainId, L2ChainId, SLChainId};

use super::*;

#[test]
fn config_schema_can_be_constructed() {
    LocalConfig::schema().unwrap();
}

fn parse_prepared_env(env: smart_config::Environment) -> (LocalConfig, ObservabilityConfig) {
    let schema = LocalConfig::schema().unwrap();
    let mut tester = Tester::new(schema);
    tester.coerce_serde_enums().coerce_variant_names();
    // Since non-prefixed vars are read using fallbacks, we need to set them via the tester.
    for (name, value) in env.iter() {
        if !name.starts_with("en_") {
            if let Some(value) = value.inner.as_plain_str() {
                tester.set_env(name.to_uppercase(), value);
            }
        }
    }

    let repo = tester.new_repository().with(env.strip_prefix("EN_"));
    let observability: ObservabilityConfig = repo.parse().unwrap();
    (LocalConfig::new(repo, false).unwrap(), observability)
}

fn assert_common_prepared_env(config: &LocalConfig, observability: &ObservabilityConfig) {
    assert_eq!(observability.log_format, "plain");
    assert!(
        observability.log_directives.contains("warn,zksync=info"),
        "{observability:?}"
    );
    assert!(observability.sentry.url.is_none(), "{observability:?}");

    assert_eq!(config.api.web3_json_rpc.http_port, 3_060);
    assert_eq!(config.api.web3_json_rpc.ws_port, 3_061);
    assert_eq!(config.api.healthcheck.port, 3_081);
    assert_eq!(
        config.db.state_keeper_db_path.as_os_str(),
        "./db/ext-node/state_keeper"
    );
    assert_eq!(
        config.db.merkle_tree.path.as_os_str(),
        "./db/ext-node/lightweight"
    );
    let postgres_url = config.secrets.database.server_url.as_ref().unwrap();
    assert!(
        postgres_url.expose_str().starts_with("postgres://"),
        "{postgres_url:?}"
    );
}

#[test]
fn parsing_prepared_mainnet_env() {
    let raw = include_str!("mainnet-config.env");
    let env = smart_config::Environment::from_dotenv("config.env", raw).unwrap();
    let (config, observability) = parse_prepared_env(env);
    assert_common_prepared_env(&config, &observability);

    assert_eq!(observability.sentry.environment.unwrap(), "zksync_mainnet");
    assert_eq!(
        config.secrets.l1.l1_rpc_url.as_ref().unwrap().expose_str(),
        "http://127.0.0.1:8545/"
    );
    assert_eq!(config.networks.l2_chain_id, L2ChainId::from(324));
    assert_eq!(config.networks.l1_chain_id, L1ChainId(1));
    assert_eq!(
        config.networks.main_node_url.expose_str(),
        "https://zksync2-mainnet.zksync.io/"
    );
}

#[test]
fn parsing_prepared_testnet_env() {
    let raw = include_str!("testnet-sepolia-config.env");
    let env = smart_config::Environment::from_dotenv("config.env", raw).unwrap();
    let (config, observability) = parse_prepared_env(env);
    assert_common_prepared_env(&config, &observability);

    assert_eq!(observability.sentry.environment.unwrap(), "zksync_testnet");
    assert_eq!(
        config.secrets.l1.l1_rpc_url.as_ref().unwrap().expose_str(),
        "http://127.0.0.1:8545/"
    );
    assert_eq!(config.networks.l2_chain_id, L2ChainId::from(300));
    assert_eq!(config.networks.l1_chain_id, L1ChainId(11155111));
    assert_eq!(
        config.networks.main_node_url.expose_str(),
        "https://sepolia.era.zksync.dev/"
    );
}

fn parse_compose_config_env(raw: &str) -> smart_config::Environment {
    let compose_config: serde_yaml::Mapping = serde_yaml::from_str(raw).unwrap();
    let env = compose_config["services"]["external-node"]["environment"]
        .as_mapping()
        .unwrap();
    let env = env.iter().map(|(name, value)| {
        let name = name.as_str().unwrap();
        let value = match value {
            serde_yaml::Value::String(val) => val.clone(),
            serde_yaml::Value::Number(val) => val.to_string(),
            serde_yaml::Value::Bool(val) => val.to_string(),
            _ => panic!("unexpected env value for env var '{name}': {value:?}"),
        };
        (name, value)
    });
    smart_config::Environment::from_iter("", env)
}

#[test]
fn parsing_mainnet_docker_compose_env() {
    let raw = include_str!("mainnet-external-node-docker-compose.yml");
    let env = parse_compose_config_env(raw);
    let (config, observability) = parse_prepared_env(env);
    assert_common_prepared_env(&config, &observability);

    assert_eq!(
        config.secrets.l1.l1_rpc_url.as_ref().unwrap().expose_str(),
        "https://ethereum-rpc.publicnode.com/"
    );
    assert_eq!(config.networks.l2_chain_id, L2ChainId::from(324));
    assert_eq!(config.networks.l1_chain_id, L1ChainId(1));
    assert_eq!(
        config.networks.main_node_url.expose_str(),
        "https://zksync2-mainnet.zksync.io/"
    );
    assert!(config.snapshot_recovery.enabled);
    let recovery_object_store = config.snapshot_recovery.object_store.unwrap();
    assert_matches!(
        &recovery_object_store.mode,
        ObjectStoreMode::GCSAnonymousReadOnly { bucket_base_url } if bucket_base_url == "zksync-era-mainnet-external-node-snapshots"
    );
}

#[test]
fn parsing_testnet_docker_compose_env() {
    let raw = include_str!("testnet-external-node-docker-compose.yml");
    let env = parse_compose_config_env(raw);
    let (config, observability) = parse_prepared_env(env);
    assert_common_prepared_env(&config, &observability);

    assert_eq!(
        config.secrets.l1.l1_rpc_url.as_ref().unwrap().expose_str(),
        "https://ethereum-sepolia-rpc.publicnode.com/"
    );
    assert_eq!(config.networks.l2_chain_id, L2ChainId::from(300));
    assert_eq!(config.networks.l1_chain_id, L1ChainId(11155111));
    assert_eq!(
        config.networks.main_node_url.expose_str(),
        "https://sepolia.era.zksync.dev/"
    );
    assert!(config.snapshot_recovery.enabled);
    let recovery_object_store = config.snapshot_recovery.object_store.unwrap();
    assert_matches!(
        &recovery_object_store.mode,
        ObjectStoreMode::GCSAnonymousReadOnly { bucket_base_url } if bucket_base_url == "zksync-era-boojnet-external-node-snapshots"
    );
}

#[test]
fn parsing_from_full_env() {
    let env = r#"
        # Observability config
        EN_PROMETHEUS_PORT=3322
        EN_PROMETHEUS_PUSHGATEWAY_URL=http://prometheus/
        EN_PROMETHEUS_PUSH_INTERVAL_MS=150
        EN_LOG_FORMAT=json
        EN_SENTRY_ENVIRONMENT="mainnet - mainnet2"
        EN_SENTRY_URL=https://example.com/new
        EN_LOG_DIRECTIVES=warn,zksync=info

        # Optional config, in the order its params were defined.
        EN_FILTERS_LIMIT=5000
        EN_SUBSCRIPTIONS_LIMIT=5000
        EN_REQ_ENTITIES_LIMIT=1000

        # FIXME: not parsed (requires idiomatic units)
        # EN_MAX_TX_SIZE_BYTES=1000000
        EN_MAX_TX_SIZE=1000000

        EN_VM_EXECUTION_CACHE_MISSES_LIMIT=1000
        EN_FEE_HISTORY_LIMIT=100
        EN_MAX_BATCH_REQUEST_SIZE=50
        EN_MAX_RESPONSE_BODY_SIZE_MB=5
        EN_MAX_RESPONSE_BODY_SIZE_OVERRIDES_MB="zks_getProof=100,eth_call=2"

        # FIXME: not parsed (requires idiomatic units)
        # EN_PUBSUB_POLLING_INTERVAL_MS=200
        EN_PUBSUB_POLLING_INTERVAL=200

        EN_MAX_NONCE_AHEAD=33
        EN_VM_CONCURRENCY_LIMIT=100
        EN_FACTORY_DEPS_CACHE_SIZE_MB=100
        EN_INITIAL_WRITES_CACHE_SIZE_MB=50
        EN_LATEST_VALUES_CACHE_SIZE_MB=200
        EN_API_NAMESPACES="zks,eth"
        EN_FILTERS_DISABLED=true

        # FIXME: not parsed (requires idiomatic units)
        # EN_MEMPOOL_CACHE_UPDATE_INTERVAL_MS=75
        EN_MEMPOOL_CACHE_UPDATE_INTERVAL=75

        EN_MEMPOOL_CACHE_SIZE=1000
        EN_EXTENDED_RPC_TRACING=true

        EN_HEALTHCHECK_SLOW_TIME_LIMIT_MS=75
        EN_HEALTHCHECK_HARD_TIME_LIMIT_MS=2500
        EN_ESTIMATE_GAS_SCALE_FACTOR=1.2
        EN_ESTIMATE_GAS_ACCEPTABLE_OVERESTIMATION=2000
        EN_ESTIMATE_GAS_OPTIMIZE_SEARCH=true
        EN_GAS_PRICE_SCALE_FACTOR=1.4

        # NEW PARAMS: From Web3RpcConfig
        EN_WEBSOCKET_REQUESTS_PER_MINUTE_LIMIT=1000
        EN_LATEST_VALUES_MAX_BLOCK_LAG=30
        EN_WHITELISTED_TOKENS_FOR_AA=0x0000000000000000000000000000000000000001

        EN_MERKLE_TREE_PROCESSING_DELAY_MS=100
        EN_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER=5
        EN_MERKLE_TREE_MAX_OPEN_FILES=1024
        EN_MERKLE_TREE_MULTI_GET_CHUNK_SIZE=1000
        EN_MERKLE_TREE_BLOCK_CACHE_SIZE_MB=4096
        EN_MERKLE_TREE_INCLUDE_INDICES_AND_FILTERS_IN_BLOCK_CACHE=true
        EN_MERKLE_TREE_MEMTABLE_CAPACITY_MB=256
        EN_MERKLE_TREE_STALLED_WRITES_TIMEOUT_SEC=15
        # MIGRATION NEEDED: was `EN_MERKLE_TREE_REPAIR_STALE_KEYS` (w/o `experimental` infix)
        EN_EXPERIMENTAL_MERKLE_TREE_REPAIR_STALE_KEYS=true
        # NEW PARAMS: In MerkleTreeConfig; not used
        EN_MERKLE_TREE_MODE=Lightweight

        EN_DATABASE_LONG_CONNECTION_THRESHOLD_MS=2500
        EN_DATABASE_SLOW_QUERY_THRESHOLD_MS=1500
        # NEW PARAMS: From PostgresConfig
        EN_DATABASE_ACQUIRE_TIMEOUT_SEC=15
        EN_DATABASE_STATEMENT_TIMEOUT_SEC=20
        # NEW PARAMS: From PostgresConfig; not used
        EN_DATABASE_MAX_CONNECTIONS_MASTER=10

        EN_L2_BLOCK_SEAL_QUEUE_CAPACITY=20
        EN_PROTECTIVE_READS_PERSISTENCE_ENABLED=true
        # NEW PARAMS: From SharedStateKeeperConfig
        EN_SAVE_CALL_TRACES=false

        EN_CONTRACTS_DIAMOND_PROXY_ADDR=0x0000000000000000000000000000000000010001
        EN_MAIN_NODE_RATE_LIMIT_RPS=150

        EN_SNAPSHOTS_RECOVERY_ENABLED=true
        EN_SNAPSHOTS_RECOVERY_POSTGRES_MAX_CONCURRENCY=5
        EN_SNAPSHOTS_OBJECT_STORE_MODE=GCSAnonymousReadOnly
        EN_SNAPSHOTS_OBJECT_STORE_BUCKET_BASE_URL=zksync-era-mainnet-external-node-snapshots
        EN_SNAPSHOTS_OBJECT_STORE_MAX_RETRIES=5
        EN_SNAPSHOTS_OBJECT_STORE_LOCAL_MIRROR_PATH=/tmp/object-store

        EN_PRUNING_ENABLED=true
        EN_PRUNING_CHUNK_SIZE=5
        EN_PRUNING_REMOVAL_DELAY_SEC=120
        EN_PRUNING_DATA_RETENTION_SEC=86400

        EN_GATEWAY_URL=https://127.0.0.1:3150/
        EN_BRIDGE_ADDRESSES_REFRESH_INTERVAL_SEC=300
        EN_TIMESTAMP_ASSERTER_MIN_TIME_TILL_END_SEC=90

        # Required config, in the order its params were defined.
        EN_L1_CHAIN_ID=8
        EN_GATEWAY_CHAIN_ID=277
        EN_L2_CHAIN_ID=270
        EN_HTTP_PORT=2950
        EN_WS_PORT=2951
        EN_HEALTHCHECK_PORT=2952
        EN_ETH_CLIENT_URL=https://127.0.0.1:8545/
        EN_MAIN_NODE_URL=https://127.0.0.1:3050/
        EN_STATE_CACHE_PATH=/db/state-keeper
        EN_MERKLE_TREE_PATH=/db/merkle-tree

        # Experimental config
        EN_EXPERIMENTAL_STATE_KEEPER_DB_BLOCK_CACHE_CAPACITY_MB=256
        EN_EXPERIMENTAL_STATE_KEEPER_DB_MAX_OPEN_FILES=512
        # MIGRATION NEEDED: Shorten `EN_EXPERIMENTAL_` -> `EN_` in the following params; considered non-breaking since params were experimental
        EN_SNAPSHOTS_RECOVERY_L1_BATCH=123
        EN_SNAPSHOTS_RECOVERY_DROP_STORAGE_KEY_PREIMAGES=true
        EN_SNAPSHOTS_RECOVERY_TREE_CHUNK_SIZE=50000
        EN_SNAPSHOTS_RECOVERY_TREE_PARALLEL_PERSISTENCE_BUFFER=5
        EN_COMMITMENT_GENERATOR_MAX_PARALLELISM=4

        # API component config
        EN_API_TREE_API_REMOTE_URL=http://tree/
        # Tree component config
        EN_TREE_API_PORT=2955
    "#;
    let env = smart_config::Environment::from_dotenv("test.env", env)
        .unwrap()
        .strip_prefix("EN_");
    test_parsing_general_config(env);
}

fn test_parsing_general_config(source: impl ConfigSource + Clone) {
    let mut tester = Tester::new(LocalConfig::schema().unwrap());
    tester.coerce_variant_names().coerce_serde_enums();
    tester
        .set_env(
            "DATABASE_URL",
            "postgres://postgres:notsecurepassword@localhost:5432/en",
        )
        .set_env("DATABASE_POOL_SIZE", "50");

    let config: PrometheusConfig = tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(config.listener_port, Some(3_322));
    assert_eq!(config.pushgateway_url.unwrap(), "http://prometheus/");
    assert_eq!(config.push_interval, Duration::from_millis(150));

    let config: ObservabilityConfig = tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(config.log_format, "json");
    assert_eq!(config.log_directives, "warn,zksync=info");
    let sentry = config.sentry;
    assert_eq!(sentry.url.unwrap(), "https://example.com/new");
    assert_eq!(sentry.environment.unwrap(), "mainnet - mainnet2");

    let config: Web3JsonRpcConfig = tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(config.filters_limit, 5_000);
    assert_eq!(config.subscriptions_limit, 5_000);
    assert_eq!(config.req_entities_limit, 1_000);
    assert_eq!(config.max_tx_size, ByteSize(1_000_000));
    assert_eq!(config.vm_execution_cache_misses_limit, Some(1_000));
    assert_eq!(config.fee_history_limit, 100);
    assert_eq!(config.max_batch_request_size, 50);
    assert_eq!(config.max_response_body_size, ByteSize(5 << 20));
    assert_eq!(
        config.max_response_body_size_overrides_mb,
        MaxResponseSizeOverrides::from_iter([
            ("zks_getProof", NonZeroUsize::new(100)),
            ("eth_call", NonZeroUsize::new(2))
        ])
    );
    assert_eq!(config.pubsub_polling_interval, Duration::from_millis(200));
    assert_eq!(config.max_nonce_ahead, 33);
    assert_eq!(config.vm_concurrency_limit, 100);
    assert_eq!(config.factory_deps_cache_size, ByteSize(100 << 20));
    assert_eq!(config.initial_writes_cache_size, ByteSize(50 << 20));
    assert_eq!(config.latest_values_cache_size, ByteSize(200 << 20));
    assert_eq!(
        config.api_namespaces,
        Some(vec!["zks".to_owned(), "eth".to_owned()])
    );
    assert!(config.filters_disabled);
    assert_eq!(
        config.mempool_cache_update_interval,
        Duration::from_millis(75)
    );
    assert_eq!(config.mempool_cache_size, 1_000);
    assert_eq!(config.estimate_gas_scale_factor, 1.2);
    assert_eq!(config.estimate_gas_acceptable_overestimation, 2_000);
    assert!(config.estimate_gas_optimize_search);
    assert_eq!(config.gas_price_scale_factor, 1.4);
    assert_eq!(config.http_port, 2_950);
    assert_eq!(config.ws_port, 2_951);

    let config: HealthCheckConfig = tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(config.slow_time_limit, Some(Duration::from_millis(75)));
    assert_eq!(config.hard_time_limit, Some(Duration::from_millis(2_500)));
    assert_eq!(config.port, 2_952);

    let config: MerkleTreeApiConfig = tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(config.port, 2955);

    let db_config: DBConfig = tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(
        db_config.state_keeper_db_path.as_os_str(),
        "/db/state-keeper"
    );

    let config = db_config.merkle_tree;
    assert_eq!(config.path.as_os_str(), "/db/merkle-tree");
    assert_eq!(config.mode, MerkleTreeMode::Lightweight);
    assert_eq!(config.processing_delay, Duration::from_millis(100));
    assert_eq!(config.max_l1_batches_per_iter, 5);
    assert_eq!(config.max_open_files, Some(NonZeroU32::new(1_024).unwrap()));
    assert_eq!(config.multi_get_chunk_size, 1_000);
    assert_eq!(config.block_cache_size, ByteSize(4 << 30));
    assert!(config.include_indices_and_filters_in_block_cache);
    assert_eq!(config.memtable_capacity, ByteSize(256 << 20));
    assert_eq!(config.stalled_writes_timeout, Duration::from_secs(15));

    let config = db_config.experimental;
    assert!(config.merkle_tree_repair_stale_keys);
    assert_eq!(
        config.state_keeper_db_block_cache_capacity,
        ByteSize(256 << 20)
    );
    assert_eq!(
        config.state_keeper_db_max_open_files,
        Some(NonZeroU32::new(512).unwrap())
    );

    let config: zksync_config::PostgresConfig =
        tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(config.max_connections, Some(50));
    assert_eq!(
        config.long_connection_threshold,
        Duration::from_millis(2_500)
    );
    assert_eq!(config.slow_query_threshold, Duration::from_millis(1_500));
    assert_eq!(config.acquire_timeout, Duration::from_secs(15));
    assert_eq!(config.statement_timeout, Duration::from_secs(20));

    let config: SharedStateKeeperConfig =
        tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(config.l2_block_seal_queue_capacity, 20);
    assert!(config.protective_reads_persistence_enabled);
    assert!(!config.save_call_traces);

    let config: SharedL1ContractsConfig =
        tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(
        config.diamond_proxy_addr.unwrap(),
        "0x0000000000000000000000000000000000010001"
            .parse()
            .unwrap()
    );

    let config: SnapshotRecoveryConfig = tester.for_config().test_complete(source.clone()).unwrap();
    assert!(config.enabled);
    assert_eq!(
        config.postgres.max_concurrency,
        NonZeroUsize::new(5).unwrap()
    );
    assert_eq!(config.l1_batch, Some(L1BatchNumber(123)));
    assert!(config.drop_storage_key_preimages);
    assert_eq!(config.tree.chunk_size, 50_000);
    assert_eq!(
        config.tree.parallel_persistence_buffer,
        Some(NonZeroUsize::new(5).unwrap())
    );
    let object_store = config.object_store.unwrap();
    let ObjectStoreMode::GCSAnonymousReadOnly { bucket_base_url } = object_store.mode else {
        panic!("unexpected object store config: {object_store:?}");
    };
    assert_eq!(
        bucket_base_url,
        "zksync-era-mainnet-external-node-snapshots"
    );
    assert_eq!(object_store.max_retries, 5);
    assert_eq!(
        object_store.local_mirror_path.unwrap().as_os_str(),
        "/tmp/object-store"
    );

    let config: PruningConfig = tester.for_config().test_complete(source.clone()).unwrap();
    assert!(config.enabled);
    assert_eq!(config.chunk_size, NonZeroU32::new(5).unwrap());
    assert_eq!(config.removal_delay, Duration::from_secs(120));
    assert_eq!(config.data_retention, Duration::from_secs(86_400));

    let config: TimestampAsserterConfig =
        tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(config.min_time_till_end, Duration::from_secs(90));

    let config: CommitmentGeneratorConfig =
        tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(config.max_parallelism, Some(NonZeroU32::new(4).unwrap()));

    let config: ENConfig = tester.for_config().test_complete(source.clone()).unwrap();
    assert_eq!(
        config.main_node_rate_limit_rps,
        NonZeroUsize::new(150).unwrap()
    );
    assert_eq!(
        config.bridge_addresses_refresh_interval,
        Duration::from_secs(300)
    );
    assert_eq!(config.l1_chain_id, L1ChainId(8));
    assert_eq!(config.gateway_chain_id, Some(SLChainId(277)));
    assert_eq!(config.l2_chain_id, L2ChainId::from(270));
    assert_eq!(config.main_node_url.expose_str(), "https://127.0.0.1:3050/");

    let secrets: Secrets = tester.for_config().test(source.clone()).unwrap();
    assert_eq!(
        secrets.database.server_url.unwrap().expose_str(),
        "postgres://postgres:notsecurepassword@localhost:5432/en"
    );
    assert_eq!(
        secrets.l1.l1_rpc_url.unwrap().expose_str(),
        "https://127.0.0.1:8545/"
    );
    assert_eq!(
        secrets.l1.gateway_rpc_url.unwrap().expose_str(),
        "https://127.0.0.1:3150/"
    );
}

#[test]
fn parsing_from_yaml() {
    let yaml = serde_yaml::from_str(include_str!("config.yaml")).unwrap();
    let yaml = Yaml::new("test.yaml", yaml).unwrap();
    test_parsing_general_config(yaml);
}

#[test]
fn avail_da_client_from_env() {
    let env = r#"
        EN_DA_CLIENT="Avail"
        EN_DA_AVAIL_CLIENT_TYPE="FullClient"
        EN_DA_BRIDGE_API_URL="localhost:54321"
        EN_DA_TIMEOUT_MS="2000"
        EN_DA_API_NODE_URL="localhost:12345"
        EN_DA_APP_ID="1"
        EN_DA_FINALITY_STATE="inBlock"
        # MIGRATION NEEDED (?): Likely wasn't parsed correctly before
        EN_DA_DISPATCH_TIMEOUT_SEC=30

        # Secrets
        EN_DA_SECRETS_SEED_PHRASE="correct horse battery staple"
        EN_DA_SECRETS_GAS_RELAY_API_KEY="key_123456"
    "#;
    let env = smart_config::Environment::from_dotenv("test.env", env)
        .unwrap()
        .strip_prefix("EN_");

    let mut tester = Tester::new(LocalConfig::schema().unwrap());
    tester.coerce_variant_names().coerce_serde_enums();

    let config: DAClientConfig = tester.for_config().test_complete(env.clone()).unwrap();
    let DAClientConfig::Avail(config) = &config else {
        panic!("unexpected config: {config:?}");
    };
    assert_eq!(config.timeout, Duration::from_secs(2));
    assert_eq!(config.bridge_api_url, "localhost:54321");
    let AvailClientConfig::FullClient(config) = &config.config else {
        panic!("unexpected config: {config:?}");
    };
    assert_eq!(config.app_id, 1);
    assert_eq!(config.api_node_url, "localhost:12345");
    assert_eq!(config.dispatch_timeout, Duration::from_secs(30));
    assert_eq!(config.finality_state, AvailFinalityState::InBlock);

    let secrets: DataAvailabilitySecrets = tester.for_config().test_complete(env.clone()).unwrap();
    let DataAvailabilitySecrets::Avail(secrets) = secrets else {
        panic!("unexpected DA secrets");
    };
    assert_eq!(
        secrets.seed_phrase.unwrap().expose_secret(),
        "correct horse battery staple"
    );
    assert_eq!(
        secrets.gas_relay_api_key.unwrap().expose_secret(),
        "key_123456"
    );
}

#[test]
fn celestia_da_client_from_env() {
    let env = r#"
        EN_DA_CLIENT="Celestia"
        EN_DA_API_NODE_URL="localhost:12345"
        EN_DA_NAMESPACE="0x1234567890abcdef"
        EN_DA_CHAIN_ID="mocha-4"
        EN_DA_TIMEOUT_MS="7000"

        # Secrets
        EN_DA_SECRETS_PRIVATE_KEY="f55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73"
    "#;
    let env = smart_config::Environment::from_dotenv("test.env", env)
        .unwrap()
        .strip_prefix("EN_");

    let mut tester = Tester::new(LocalConfig::schema().unwrap());
    tester.coerce_variant_names().coerce_serde_enums();

    let config: DAClientConfig = tester.for_config().test_complete(env.clone()).unwrap();
    let DAClientConfig::Celestia(config) = &config else {
        panic!("unexpected config: {config:?}");
    };
    assert_eq!(config.api_node_url, "localhost:12345");
    assert_eq!(config.namespace, "0x1234567890abcdef");
    assert_eq!(config.chain_id, "mocha-4");
    assert_eq!(config.timeout, Duration::from_secs(7));

    let secrets: DataAvailabilitySecrets = tester.for_config().test_complete(env.clone()).unwrap();
    let DataAvailabilitySecrets::Celestia(secrets) = secrets else {
        panic!("unexpected DA secrets");
    };
    assert_eq!(
        secrets.private_key.expose_secret(),
        "f55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73"
    );
}

#[test]
fn eigen_da_client_from_env() {
    let env = r#"
        EN_DA_CLIENT="Eigen"
        EN_DA_DISPERSER_RPC="http://localhost:8080"
        EN_DA_SETTLEMENT_LAYER_CONFIRMATION_DEPTH=0
        EN_DA_EIGENDA_ETH_RPC="http://localhost:8545"
        EN_DA_EIGENDA_SVC_MANAGER_ADDRESS="0x0000000000000000000000000000000000000123"
        EN_DA_WAIT_FOR_FINALIZATION=true
        EN_DA_AUTHENTICATED=false
        EN_DA_POINTS_SOURCE="Path"
        EN_DA_POINTS_PATH="resources"
        EN_DA_CUSTOM_QUORUM_NUMBERS="2"

        # Secrets
        EN_DA_SECRETS_PRIVATE_KEY="f55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73"
    "#;
    let env = smart_config::Environment::from_dotenv("test.env", env)
        .unwrap()
        .strip_prefix("EN_");

    let mut tester = Tester::new(LocalConfig::schema().unwrap());
    tester.coerce_variant_names().coerce_serde_enums();

    let config: DAClientConfig = tester.for_config().test_complete(env.clone()).unwrap();
    let DAClientConfig::Eigen(config) = &config else {
        panic!("unexpected config: {config:?}");
    };
    assert_eq!(config.disperser_rpc, "http://localhost:8080");
    assert_eq!(config.settlement_layer_confirmation_depth, 0);
    assert_eq!(
        config.eigenda_eth_rpc.as_ref().unwrap().expose_str(),
        "http://localhost:8545/"
    );
    assert_eq!(
        config.eigenda_svc_manager_address,
        "0x0000000000000000000000000000000000000123"
            .parse()
            .unwrap()
    );
    assert!(config.wait_for_finalization);
    assert!(!config.authenticated);
    let PointsSource::Path { path } = &config.points else {
        panic!("unexpected points: {config:?}");
    };
    assert_eq!(path, "resources");
    assert_eq!(config.custom_quorum_numbers, [2]);

    let secrets: DataAvailabilitySecrets = tester.for_config().test_complete(env.clone()).unwrap();
    let DataAvailabilitySecrets::Eigen(secrets) = secrets else {
        panic!("unexpected DA secrets");
    };
    assert_eq!(
        secrets.private_key.expose_secret(),
        "f55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73"
    );
}
