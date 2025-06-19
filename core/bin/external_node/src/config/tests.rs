//! Tests for EN configuration.

use std::collections::HashMap;

use assert_matches::assert_matches;
use zksync_config::configs::observability::LogFormat;

use super::*;
use crate::config::observability::ObservabilityENConfig;

#[derive(Debug)]
struct MockEnvironment(HashMap<&'static str, &'static str>);

impl MockEnvironment {
    pub fn new(vars: &[(&'static str, &'static str)]) -> Self {
        Self(vars.iter().copied().collect())
    }
}

impl ConfigurationSource for MockEnvironment {
    type Vars<'a> = Box<dyn Iterator<Item = (OsString, OsString)> + 'a>;

    fn vars(&self) -> Self::Vars<'_> {
        Box::new(
            self.0
                .iter()
                .map(|(&name, &value)| (OsString::from(name), OsString::from(value))),
        )
    }

    fn var(&self, name: &str) -> Option<String> {
        self.0.get(name).copied().map(str::to_owned)
    }
}

#[test]
fn parsing_observability_config() {
    let mut env_vars = MockEnvironment::new(&[
        ("EN_PROMETHEUS_PORT", "3322"),
        ("MISC_SENTRY_URL", "https://example.com/"),
        ("EN_SENTRY_ENVIRONMENT", "mainnet - mainnet2"),
    ]);
    let config = ObservabilityENConfig::new(&env_vars).unwrap();
    assert_eq!(config.prometheus_port, Some(3322));
    assert_eq!(config.sentry_url.unwrap(), "https://example.com/");
    assert_eq!(config.sentry_environment.unwrap(), "mainnet - mainnet2");
    assert_matches!(config.log_format, LogFormat::Plain);
    assert_eq!(config.prometheus_push_interval_ms, 10_000);

    env_vars.0.insert("MISC_LOG_FORMAT", "json");
    let config = ObservabilityENConfig::new(&env_vars).unwrap();
    assert_matches!(config.log_format, LogFormat::Json);

    // If both the canonical and obsolete vars are specified, the canonical one should prevail.
    env_vars.0.insert("EN_LOG_FORMAT", "plain");
    env_vars
        .0
        .insert("EN_SENTRY_URL", "https://example.com/new");
    let config = ObservabilityENConfig::new(&env_vars).unwrap();
    assert_matches!(config.log_format, LogFormat::Plain);
    assert_eq!(config.sentry_url.unwrap(), "https://example.com/new");
}

#[test]
fn using_unset_sentry_url() {
    let env_vars = MockEnvironment::new(&[("MISC_SENTRY_URL", "unset")]);
    let config = ObservabilityENConfig::new(&env_vars).unwrap();
    if let Err(err) = config.build_observability() {
        // Global tracer may be installed by another test, but the logic shouldn't fail before that.
        assert!(format!("{err:?}").contains("global tracer"), "{err:?}");
    }
}

#[test]
fn parsing_optional_config_from_empty_env() {
    let config: OptionalENConfig = envy::prefixed("EN_").from_iter([]).unwrap();
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
    assert_eq!(config.estimate_gas_scale_factor, 1.3);
    assert_eq!(config.gas_price_scale_factor, 1.5);
    assert_eq!(config.estimate_gas_acceptable_overestimation, 5000);
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
        ("EN_TIMESTAMP_ASSERTER_MIN_TIME_TILL_END_SEC", "2"),
    ];
    let env_vars = env_vars
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()));

    let config: OptionalENConfig = envy::prefixed("EN_").from_iter(env_vars).unwrap();
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
            ("zks_getProof", NonZeroUsize::new(100 * BYTES_IN_MEGABYTE)),
            ("eth_call", NonZeroUsize::new(2 * BYTES_IN_MEGABYTE))
        ])
    );
}

#[test]
fn parsing_renamed_optional_parameters() {
    let env_vars = [
        ("EN_MAX_TX_SIZE_BYTES", "100000"),
        ("EN_PUBSUB_POLLING_INTERVAL_MS", "250"),
        ("EN_MEMPOOL_CACHE_UPDATE_INTERVAL_MS", "100"),
        ("EN_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER", "15"),
    ];
    let env_vars = env_vars
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()));

    let config: OptionalENConfig = envy::prefixed("EN_").from_iter(env_vars).unwrap();
    assert_eq!(config.max_tx_size_bytes, 100_000);
    assert_eq!(config.pubsub_polling_interval_ms, 250);
    assert_eq!(config.mempool_cache_update_interval_ms, 100);
    assert_eq!(config.merkle_tree_max_l1_batches_per_iter, 15);
}

#[test]
fn parsing_experimental_config_from_empty_env() {
    let config: ExperimentalENConfig = envy::prefixed("EN_EXPERIMENTAL_").from_iter([]).unwrap();
    assert_eq!(config.state_keeper_db_block_cache_capacity(), 128 << 20);
    assert_eq!(config.state_keeper_db_max_open_files, None);
}

#[test]
fn parsing_experimental_config_from_env() {
    let env_vars = [
        (
            "EN_EXPERIMENTAL_STATE_KEEPER_DB_BLOCK_CACHE_CAPACITY_MB",
            "64",
        ),
        ("EN_EXPERIMENTAL_STATE_KEEPER_DB_MAX_OPEN_FILES", "100"),
    ];
    let env_vars = env_vars
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()));

    let config: ExperimentalENConfig = envy::prefixed("EN_EXPERIMENTAL_")
        .from_iter(env_vars)
        .unwrap();
    assert_eq!(config.state_keeper_db_block_cache_capacity(), 64 << 20);
    assert_eq!(config.state_keeper_db_max_open_files, NonZeroU32::new(100));
}
