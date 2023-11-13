//! Defined protobuf mapping for the config files.
//! It allows to encode the configs using:
//! * protobuf binary format
//! * protobuf text format
//! * protobuf json format
use anyhow::Context as _;
use std::convert::{TryFrom as _, TryInto as _};
use zksync_config::configs::{
    api::ContractVerificationApiConfig, api::HealthCheckConfig, api::MerkleTreeApiConfig,
    api::Web3JsonRpcConfig, AlertsConfig, ApiConfig, PrometheusConfig,
};
use zksync_protobuf::required;

pub mod proto;

/// Trait reverse to `zksync_protobuf::ProtoFmt` for cases where
/// you would like to specify a custom proto encoding for an externally defined type.
trait ProtoRepr<T>: prost::Message {
    fn read(r: &Self) -> anyhow::Result<T>;
    fn build(this: &T) -> Self;
}

fn read_required_repr<T, P: ProtoRepr<T>>(field: &Option<P>) -> anyhow::Result<T> {
    ProtoRepr::read(field.as_ref().context("missing field")?)
}

impl ProtoRepr<AlertsConfig> for proto::Alerts {
    fn read(r: &Self) -> anyhow::Result<AlertsConfig> {
        Ok(AlertsConfig {
            sporadic_crypto_errors_substrs: r.sporadic_crypto_errors_substrs.clone(),
        })
    }

    fn build(this: &AlertsConfig) -> Self {
        Self {
            sporadic_crypto_errors_substrs: this.sporadic_crypto_errors_substrs.clone(),
        }
    }
}

impl ProtoRepr<PrometheusConfig> for proto::Prometheus {
    fn read(r: &Self) -> anyhow::Result<PrometheusConfig> {
        Ok(PrometheusConfig {
            listener_port: required(&r.listener_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("listener_port")?,
            pushgateway_url: required(&r.pushgateway_url)
                .context("pushgateway_url")?
                .clone(),
            push_interval_ms: r.push_interval_ms,
        })
    }

    fn build(this: &PrometheusConfig) -> Self {
        Self {
            listener_port: Some(this.listener_port.into()),
            pushgateway_url: Some(this.pushgateway_url.clone()),
            push_interval_ms: this.push_interval_ms,
        }
    }
}

impl ProtoRepr<ApiConfig> for proto::Api {
    fn read(r: &Self) -> anyhow::Result<ApiConfig> {
        Ok(ApiConfig {
            web3_json_rpc: read_required_repr(&r.web3_json_rpc).context("web3_json_rpc")?,
            contract_verification: read_required_repr(&r.contract_verification)
                .context("contract_verification")?,
            prometheus: read_required_repr(&r.prometheus).context("prometheus")?,
            healthcheck: read_required_repr(&r.healthcheck).context("healthcheck")?,
            merkle_tree: read_required_repr(&r.merkle_tree).context("merkle_tree")?,
        })
    }

    fn build(this: &ApiConfig) -> Self {
        Self {
            web3_json_rpc: Some(ProtoRepr::build(&this.web3_json_rpc)),
            contract_verification: Some(ProtoRepr::build(&this.contract_verification)),
            prometheus: Some(ProtoRepr::build(&this.prometheus)),
            healthcheck: Some(ProtoRepr::build(&this.healthcheck)),
            merkle_tree: Some(ProtoRepr::build(&this.merkle_tree)),
        }
    }
}

impl ProtoRepr<Web3JsonRpcConfig> for proto::Web3JsonRpc {
    fn read(r: &Self) -> anyhow::Result<Web3JsonRpcConfig> {
        Ok(Web3JsonRpcConfig {
            http_port: required(&r.http_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("http_port")?,
            http_url: required(&r.http_url).context("http_url")?.clone(),
            ws_port: required(&r.ws_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("ws_port")?,
            ws_url: required(&r.ws_url).context("ws_url")?.clone(),
            req_entities_limit: r.req_entities_limit,
            filters_limit: r.filters_limit,
            subscriptions_limit: r.subscriptions_limit,
            pubsub_polling_interval: r.pubsub_polling_interval,
            threads_per_server: *required(&r.threads_per_server).context("threads_per_server")?,
            max_nonce_ahead: *required(&r.max_nonce_ahead).context("max_nonce_ahead")?,
            gas_price_scale_factor: *required(&r.gas_price_scale_factor)
                .context("gas_price_scale_factor")?,
            transactions_per_sec_limit: r.transactions_per_sec_limit,
            request_timeout: r.request_timeout,
            account_pks: match &r.account_pks {
                None => None,
                Some(r) => {
                    let mut keys = vec![];
                    for (i, k) in r.keys.iter().enumerate() {
                        keys.push(
                            <[u8; 32]>::try_from(&k[..])
                                .with_context(|| format!("keys[{i}]"))?
                                .into(),
                        );
                    }
                    Some(keys)
                }
            },
            estimate_gas_scale_factor: *required(&r.estimate_gas_scale_factor)
                .context("estimate_gas_scale_factor")?,
            estimate_gas_acceptable_overestimation: *required(
                &r.estimate_gas_acceptable_overestimation,
            )
            .context("acceptable_overestimation")?,
            max_tx_size: required(&r.max_tx_size)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_tx_size")?,
            vm_execution_cache_misses_limit: r
                .vm_execution_cache_misses_limit
                .map(|x| x.try_into())
                .transpose()
                .context("vm_execution_cache_misses_limit")?,
            vm_concurrency_limit: r
                .vm_concurrency_limit
                .map(|x| x.try_into())
                .transpose()
                .context("vm_concurrency_limit")?,
            factory_deps_cache_size_mb: r
                .factory_deps_cache_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("factory_deps_cache_size_mb")?,
            initial_writes_cache_size_mb: r
                .initial_writes_cache_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("initial_writes_cache_size_mb")?,
            latest_values_cache_size_mb: r
                .latest_values_cache_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("latests_values_cache_size_mb")?,
            http_threads: r.http_threads,
            ws_threads: r.ws_threads,
            fee_history_limit: r.fee_history_limit,
            max_batch_request_size: r
                .max_batch_request_size
                .map(|x| x.try_into())
                .transpose()
                .context("max_batch_requres_size")?,
            max_response_body_size_mb: r
                .max_response_body_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("max_response_body_size_mb")?,
            websocket_requests_per_minute_limit: r.websocket_requests_per_minute_limit,
        })
    }
    fn build(this: &Web3JsonRpcConfig) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            http_url: Some(this.http_url.clone()),
            ws_port: Some(this.ws_port.into()),
            ws_url: Some(this.ws_url.clone()),
            req_entities_limit: this.req_entities_limit,
            filters_limit: this.filters_limit,
            subscriptions_limit: this.subscriptions_limit,
            pubsub_polling_interval: this.pubsub_polling_interval,
            threads_per_server: Some(this.threads_per_server),
            max_nonce_ahead: Some(this.max_nonce_ahead),
            gas_price_scale_factor: Some(this.gas_price_scale_factor),
            transactions_per_sec_limit: this.transactions_per_sec_limit,
            request_timeout: this.request_timeout,
            account_pks: this.account_pks.as_ref().map(|keys| proto::PrivateKeys {
                keys: keys.iter().map(|k| k.as_bytes().into()).collect(),
            }),
            estimate_gas_scale_factor: Some(this.estimate_gas_scale_factor),
            estimate_gas_acceptable_overestimation: Some(
                this.estimate_gas_acceptable_overestimation,
            ),
            max_tx_size: Some(this.max_tx_size.try_into().unwrap()),
            vm_execution_cache_misses_limit: this
                .vm_execution_cache_misses_limit
                .map(|x| x.try_into().unwrap()),
            vm_concurrency_limit: this.vm_concurrency_limit.map(|x| x.try_into().unwrap()),
            factory_deps_cache_size_mb: this
                .factory_deps_cache_size_mb
                .map(|x| x.try_into().unwrap()),
            initial_writes_cache_size_mb: this
                .initial_writes_cache_size_mb
                .map(|x| x.try_into().unwrap()),
            latest_values_cache_size_mb: this
                .latest_values_cache_size_mb
                .map(|x| x.try_into().unwrap()),
            http_threads: this.http_threads,
            ws_threads: this.ws_threads,
            fee_history_limit: this.fee_history_limit,
            max_batch_request_size: this.max_batch_request_size.map(|x| x.try_into().unwrap()),
            max_response_body_size_mb: this
                .max_response_body_size_mb
                .map(|x| x.try_into().unwrap()),
            websocket_requests_per_minute_limit: this.websocket_requests_per_minute_limit,
        }
    }
}

impl ProtoRepr<ContractVerificationApiConfig> for proto::ContractVerificationApi {
    fn read(r: &Self) -> anyhow::Result<ContractVerificationApiConfig> {
        Ok(ContractVerificationApiConfig {
            port: required(&r.port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("port")?,
            url: required(&r.url).context("url")?.clone(),
            threads_per_server: *required(&r.threads_per_server).context("threads_per_server")?,
        })
    }
    fn build(this: &ContractVerificationApiConfig) -> Self {
        Self {
            port: Some(this.port.into()),
            url: Some(this.url.clone()),
            threads_per_server: Some(this.threads_per_server),
        }
    }
}

impl ProtoRepr<HealthCheckConfig> for proto::HealthCheck {
    fn read(r: &Self) -> anyhow::Result<HealthCheckConfig> {
        Ok(HealthCheckConfig {
            port: required(&r.port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("port")?,
        })
    }
    fn build(this: &HealthCheckConfig) -> Self {
        Self {
            port: Some(this.port.into()),
        }
    }
}

impl ProtoRepr<MerkleTreeApiConfig> for proto::MerkleTreeApi {
    fn read(r: &Self) -> anyhow::Result<MerkleTreeApiConfig> {
        Ok(MerkleTreeApiConfig {
            port: required(&r.port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("port")?,
        })
    }
    fn build(this: &MerkleTreeApiConfig) -> Self {
        Self {
            port: Some(this.port.into()),
        }
    }
}
