//! Tests for EN configuration.

use super::*;

#[test]
fn parsing_optional_config_from_empty_env() {
    let config: OptionalENConfig = envy::prefixed("EN_").from_iter([]).unwrap();
    assert_eq!(config.filters_limit, 10_000);
    assert_eq!(config.subscriptions_limit, 10_000);
    assert_eq!(config.fee_history_limit, 1_024);
    assert_eq!(config.polling_interval(), Duration::from_millis(200));
    assert_eq!(config.max_tx_size, 1_000_000);
    assert_eq!(
        config.metadata_calculator_delay(),
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
    assert_eq!(config.max_response_body_size(), 10 * BYTES_IN_MEGABYTE);
}

#[test]
fn parsing_optional_config_from_env() {
    let env_vars = [
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
    ];
    let env_vars = env_vars
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()));

    let config: OptionalENConfig = envy::prefixed("EN_").from_iter(env_vars).unwrap();
    assert_eq!(config.filters_limit, 5_000);
    assert_eq!(config.subscriptions_limit, 20_000);
    assert_eq!(config.fee_history_limit, 1_000);
    assert_eq!(config.polling_interval(), Duration::from_millis(500));
    assert_eq!(config.max_tx_size, BYTES_IN_MEGABYTE);
    assert_eq!(
        config.metadata_calculator_delay(),
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
    assert_eq!(config.max_response_body_size(), BYTES_IN_MEGABYTE);
}
