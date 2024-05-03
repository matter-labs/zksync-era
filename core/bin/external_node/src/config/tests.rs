//! Tests for EN configuration.

use std::path::Path;

use super::*;

#[test]
fn parsing_optional_config_from_empty_env() {
    let schema = ConfigSchema::default().insert::<OptionalENConfig>("");
    let config: OptionalENConfig = Environment::default()
        .parser(Envy, &schema)
        .parse()
        .unwrap();
    assert_default_optional_config(&config);
}

fn assert_default_optional_config(config: &OptionalENConfig) {
    assert_eq!(config.filters_limit, 10_000);
    assert_eq!(config.subscriptions_limit, 10_000);
    assert_eq!(config.fee_history_limit, 1_024);
    assert_eq!(config.polling_interval(), Duration::from_millis(200));
    assert_eq!(config.max_tx_size_bytes, 1_000_000);
    assert_eq!(
        config.merkle_tree_processing_delay(),
        Duration::from_millis(100)
    );
    assert_eq!(config.max_nonce_ahead, 50);
    assert_eq!(config.estimate_gas_scale_factor, 1.2);
    assert_eq!(config.vm_concurrency_limit, 2_048);
    assert_eq!(config.factory_deps_cache_size(), 128 * BYTES_IN_MEGABYTE);
    assert_eq!(config.latest_values_cache_size(), 128 * BYTES_IN_MEGABYTE);
    assert_eq!(config.merkle_tree_multi_get_chunk_size, 500);
    assert_eq!(
        config.merkle_tree_block_cache_size(),
        128 * BYTES_IN_MEGABYTE
    );
    assert_eq!(
        config.max_response_body_size().global,
        10 * BYTES_IN_MEGABYTE
    );
    assert_eq!(
        config.max_response_body_size().overrides,
        MaxResponseSizeOverrides::empty()
    );
    assert_eq!(
        config.l1_batch_commit_data_generator_mode,
        L1BatchCommitDataGeneratorMode::Rollup
    );
}

#[test]
fn parsing_optional_config_from_env() {
    let env_vars = [
        ("EN_FILTERS_DISABLED", "true"),
        ("EN_FILTERS_LIMIT", "5000"),
        ("EN_SUBSCRIPTIONS_LIMIT", "20000"),
        ("EN_FEE_HISTORY_LIMIT", "1000"),
        ("EN_PUBSUB_POLLING_INTERVAL", "500"),
        ("EN_MAX_TX_SIZE", "1048576"),
        ("EN_METADATA_CALCULATOR_DELAY", "50"),
        ("EN_MAX_NONCE_AHEAD", "100"),
        ("EN_ESTIMATE_GAS_SCALE_FACTOR", "1.5"),
        ("EN_VM_CONCURRENCY_LIMIT", "1000"),
        ("EN_FACTORY_DEPS_CACHE_SIZE_MB", "64"),
        ("EN_LATEST_VALUES_CACHE_SIZE_MB", "50"),
        ("EN_MERKLE_TREE_MULTI_GET_CHUNK_SIZE", "1000"),
        ("EN_MERKLE_TREE_BLOCK_CACHE_SIZE_MB", "32"),
        ("EN_MAX_RESPONSE_BODY_SIZE_MB", "1"),
        (
            "EN_MAX_RESPONSE_BODY_SIZE_OVERRIDES_MB",
            "zks_getProof=100,eth_call=2",
        ),
        ("EN_L1_BATCH_COMMIT_DATA_GENERATOR_MODE", "Validium"),
    ];

    let schema = ConfigSchema::default().insert::<OptionalENConfig>("");
    let config: OptionalENConfig = Environment::from_iter("EN_", env_vars)
        .parser(Envy, &schema)
        .parse()
        .unwrap();
    assert!(config.filters_disabled);
    assert_eq!(config.filters_limit, 5_000);
    assert_eq!(config.subscriptions_limit, 20_000);
    assert_eq!(config.fee_history_limit, 1_000);
    assert_eq!(config.polling_interval(), Duration::from_millis(500));
    assert_eq!(config.max_tx_size_bytes, BYTES_IN_MEGABYTE);
    assert_eq!(
        config.merkle_tree_processing_delay(),
        Duration::from_millis(50)
    );
    assert_eq!(config.max_nonce_ahead, 100);
    assert_eq!(config.estimate_gas_scale_factor, 1.5);
    assert_eq!(config.vm_concurrency_limit, 1_000);
    assert_eq!(config.factory_deps_cache_size(), 64 * BYTES_IN_MEGABYTE);
    assert_eq!(config.latest_values_cache_size(), 50 * BYTES_IN_MEGABYTE);
    assert_eq!(config.merkle_tree_multi_get_chunk_size, 1_000);
    assert_eq!(
        config.merkle_tree_block_cache_size(),
        32 * BYTES_IN_MEGABYTE
    );
    let max_response_size = config.max_response_body_size();
    assert_eq!(max_response_size.global, BYTES_IN_MEGABYTE);
    assert_eq!(
        max_response_size.overrides,
        MaxResponseSizeOverrides::from_iter([
            (
                "zks_getProof",
                NonZeroUsize::new(100 * BYTES_IN_MEGABYTE).unwrap()
            ),
            (
                "eth_call",
                NonZeroUsize::new(2 * BYTES_IN_MEGABYTE).unwrap()
            )
        ])
    );
    assert_eq!(
        config.l1_batch_commit_data_generator_mode,
        L1BatchCommitDataGeneratorMode::Validium
    );
}

#[test]
fn parsing_experimental_config_from_empty_env() {
    let schema = ConfigSchema::default().insert::<ExperimentalENConfig>("experimental");
    let config: ExperimentalENConfig = Environment::default()
        .parser(Envy, &schema)
        .parse()
        .unwrap();
    assert_default_experimental_config(&config);
}

fn assert_default_experimental_config(config: &ExperimentalENConfig) {
    assert_eq!(config.state_keeper_db_block_cache_capacity(), 128 << 20);
    assert_eq!(config.state_keeper_db_max_open_files, None);
    assert!(!config.pruning_enabled);
    assert!(!config.snapshots_recovery_enabled);
}

#[test]
fn parsing_experimental_config_from_env() {
    let env_vars = [
        (
            "EN_EXPERIMENTAL_STATE_KEEPER_DB_BLOCK_CACHE_CAPACITY_MB",
            "64",
        ),
        ("EN_EXPERIMENTAL_STATE_KEEPER_DB_MAX_OPEN_FILES", "100"),
        ("EN_SNAPSHOTS_RECOVERY_ENABLED", "true"),
        ("EN_SNAPSHOTS_RECOVERY_POSTGRES_MAX_CONCURRENCY", "10"),
        // Test a mix of original and aliased params
        ("EN_EXPERIMENTAL_PRUNING_ENABLED", "true"),
        ("EN_PRUNING_CHUNK_SIZE", "5"),
        ("EN_EXPERIMENTAL_PRUNING_REMOVAL_DELAY_SEC", "90"),
    ];
    let schema = ConfigSchema::default().insert_aliased::<ExperimentalENConfig>(
        "experimental",
        [Alias::prefix("").exclude(|name| name.starts_with("state_keeper_"))],
    );
    let config: ExperimentalENConfig = Environment::from_iter("EN_", env_vars)
        .parser(Envy, &schema)
        .parse()
        .unwrap();

    assert_eq!(config.state_keeper_db_block_cache_capacity(), 64 << 20);
    assert_eq!(config.state_keeper_db_max_open_files, NonZeroU32::new(100));
    assert!(config.snapshots_recovery_enabled);
    assert_eq!(
        config.snapshots_recovery_postgres_max_concurrency,
        NonZeroUsize::new(10).unwrap()
    );
    assert!(config.pruning_enabled);
    assert_eq!(config.pruning_chunk_size, 5);
    assert_eq!(config.pruning_removal_delay(), Duration::from_secs(90));
}

// All environment variables necessary to initialize the node.
const REQUIRED_VARS: &[(&str, &str)] = &[
    ("EN_MAIN_NODE_URL", "http://127.0.0.1:3050"),
    ("EN_ETH_CLIENT_URL", "http://127.0.0.1:8545"),
    ("EN_HTTP_PORT", "3060"),
    ("EN_WS_PORT", "3061"),
    ("EN_HEALTHCHECK_PORT", "3080"),
    (
        "EN_DATABASE_URL",
        "postgres://postgres:notsecurepassword@localhost/zksync_local_ext_node",
    ),
    ("EN_DATABASE_POOL_SIZE", "50"),
    ("EN_STATE_CACHE_PATH", "./db/ext-node/state_keeper"),
    ("EN_MERKLE_TREE_PATH", "./db/ext-node/lightweight"),
];

#[test]
fn parsing_minimal_entire_configuration() {
    let schema = ExternalNodeConfig::schema();
    let parser = Environment::from_iter("EN_", REQUIRED_VARS.iter().copied()).parser(Envy, &schema);
    let required: RequiredENConfig = parser.parse().unwrap();
    assert_required_config(&required);

    let database: PostgresConfig = parser.parse().unwrap();
    assert_eq!(
        *database.database_url.expose_url(),
        "postgres://postgres:notsecurepassword@localhost/zksync_local_ext_node"
            .parse()
            .unwrap()
    );
    assert_eq!(database.database_pool_size, 50);

    let optional: OptionalENConfig = parser.parse().unwrap();
    assert_default_optional_config(&optional);

    let experimental: ExperimentalENConfig = parser.parse().unwrap();
    assert_default_experimental_config(&experimental);
}

fn assert_required_config(config: &RequiredENConfig) {
    assert_eq!(
        *config.main_node_url.expose_url(),
        "http://127.0.0.1:3050".parse().unwrap()
    );
    assert_eq!(
        *config.eth_client_url.expose_url(),
        "http://127.0.0.1:8545".parse().unwrap()
    );
    assert_eq!(config.http_port, 3060);
    assert_eq!(config.ws_port, 3061);
    assert_eq!(config.healthcheck_port, 3080);
    assert_eq!(
        config.state_cache_path,
        Path::new("./db/ext-node/state_keeper")
    );
    assert_eq!(
        config.merkle_tree_path,
        Path::new("./db/ext-node/lightweight")
    );
}

#[test]
fn parsing_entire_configuration() {
    let schema = ExternalNodeConfig::schema();
    let env_vars = REQUIRED_VARS.iter().copied().chain([
        ("EN_FILTERS_DISABLED", "true"),
        ("EN_VM_CONCURRENCY_LIMIT", "64"),
        ("EN_MERKLE_TREE_PROCESSING_DELAY_MS", "50"),
        ("EN_PRUNING_ENABLED", "true"),
        ("EN_EXPERIMENTAL_PRUNING_CHUNK_SIZE", "3"),
        ("EN_EXPERIMENTAL_PRUNING_REMOVAL_DELAY_SEC", "120"),
    ]);
    let parser = Environment::from_iter("EN_", env_vars).parser(Envy, &schema);
    let required: RequiredENConfig = parser.parse().unwrap();
    assert_required_config(&required);

    let database: PostgresConfig = parser.parse().unwrap();
    assert_eq!(
        *database.database_url.expose_url(),
        "postgres://postgres:notsecurepassword@localhost/zksync_local_ext_node"
            .parse()
            .unwrap()
    );
    assert_eq!(database.database_pool_size, 50);

    let optional: OptionalENConfig = parser.parse().unwrap();
    assert!(optional.filters_disabled);
    assert_eq!(optional.vm_concurrency_limit, 64);
    assert_eq!(
        optional.merkle_tree_processing_delay(),
        Duration::from_millis(50)
    );

    let experimental: ExperimentalENConfig = parser.parse().unwrap();
    assert!(experimental.pruning_enabled);
    assert_eq!(experimental.pruning_chunk_size, 3);
    assert_eq!(
        experimental.pruning_removal_delay(),
        Duration::from_secs(120)
    );
}
