//! Tests for EN configuration.

use super::*;

#[test]
fn parsing_optional_config_from_empty_env() {
    let schema = ConfigSchema::default().insert::<OptionalENConfig>("");
    let config: OptionalENConfig = Environment::default()
        .parser(Envy, &schema)
        .parse()
        .unwrap();

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
