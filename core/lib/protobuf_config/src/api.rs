use std::num::{NonZeroU32, NonZeroUsize};

use anyhow::Context as _;
use zksync_config::configs::{api, ApiConfig};
use zksync_protobuf::{
    repr::{read_required_repr, ProtoRepr},
    required,
};

use crate::{parse_h160, proto::api as proto};

impl ProtoRepr for proto::Api {
    type Type = ApiConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            web3_json_rpc: read_required_repr(&self.web3_json_rpc).context("web3_json_rpc")?,
            prometheus: read_required_repr(&self.prometheus).context("prometheus")?,
            healthcheck: read_required_repr(&self.healthcheck).context("healthcheck")?,
            merkle_tree: read_required_repr(&self.merkle_tree).context("merkle_tree")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            web3_json_rpc: Some(ProtoRepr::build(&this.web3_json_rpc)),
            prometheus: Some(ProtoRepr::build(&this.prometheus)),
            healthcheck: Some(ProtoRepr::build(&this.healthcheck)),
            merkle_tree: Some(ProtoRepr::build(&this.merkle_tree)),
        }
    }
}

impl ProtoRepr for proto::Web3JsonRpc {
    type Type = api::Web3JsonRpcConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        let max_response_body_size_overrides_mb = self
            .max_response_body_size_overrides
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let size_mb = if let Some(size_mb) = entry.size_mb {
                    let size_mb =
                        usize::try_from(size_mb).with_context(|| format!("[{i}].size_mb"))?;
                    NonZeroUsize::new(size_mb).with_context(|| format!("[{i}].size_mb is zero"))?
                } else {
                    NonZeroUsize::MAX
                };
                Ok((
                    entry
                        .method
                        .clone()
                        .with_context(|| format!("[{i}].method"))?,
                    size_mb,
                ))
            })
            .collect::<anyhow::Result<_>>()
            .context("max_response_body_size_overrides")?;
        let api_namespaces = if self.api_namespaces.is_empty() {
            None
        } else {
            Some(self.api_namespaces.clone())
        };

        Ok(Self::Type {
            http_port: required(&self.http_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("http_port")?,
            ws_port: required(&self.ws_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("ws_port")?,
            req_entities_limit: self.req_entities_limit,
            filters_disabled: self.filters_disabled.unwrap_or(false),
            filters_limit: self.filters_limit,
            subscriptions_limit: self.subscriptions_limit,
            pubsub_polling_interval: self.pubsub_polling_interval,
            max_nonce_ahead: *required(&self.max_nonce_ahead).context("max_nonce_ahead")?,
            gas_price_scale_factor: *required(&self.gas_price_scale_factor)
                .context("gas_price_scale_factor")?,
            estimate_gas_scale_factor: *required(&self.estimate_gas_scale_factor)
                .context("estimate_gas_scale_factor")?,
            estimate_gas_acceptable_overestimation: *required(
                &self.estimate_gas_acceptable_overestimation,
            )
            .context("acceptable_overestimation")?,
            estimate_gas_optimize_search: self.estimate_gas_optimize_search.unwrap_or(false),
            max_tx_size: required(&self.max_tx_size)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_tx_size")?,
            vm_execution_cache_misses_limit: self
                .vm_execution_cache_misses_limit
                .map(|x| x.try_into())
                .transpose()
                .context("vm_execution_cache_misses_limit")?,
            vm_concurrency_limit: self
                .vm_concurrency_limit
                .map(|x| x.try_into())
                .transpose()
                .context("vm_concurrency_limit")?,
            factory_deps_cache_size_mb: self
                .factory_deps_cache_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("factory_deps_cache_size_mb")?,
            initial_writes_cache_size_mb: self
                .initial_writes_cache_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("initial_writes_cache_size_mb")?,
            latest_values_cache_size_mb: self
                .latest_values_cache_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("latest_values_cache_size_mb")?,
            latest_values_max_block_lag: self
                .latest_values_max_block_lag
                .map(|x| x.try_into())
                .transpose()
                .context("latest_values_max_block_lag")?,
            fee_history_limit: self.fee_history_limit,
            max_batch_request_size: self
                .max_batch_request_size
                .map(|x| x.try_into())
                .transpose()
                .context("max_batch_request_size")?,
            max_response_body_size_mb: self
                .max_response_body_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("max_response_body_size_mb")?,
            max_response_body_size_overrides_mb,
            websocket_requests_per_minute_limit: self
                .websocket_requests_per_minute_limit
                .map(|x| x.try_into())
                .transpose()
                .context("websocket_requests_per_minute_limit")?,
            tree_api_url: self.tree_api_url.clone(),
            mempool_cache_update_interval: self.mempool_cache_update_interval,
            mempool_cache_size: self
                .mempool_cache_size
                .map(|x| x.try_into())
                .transpose()
                .context("mempool_cache_size")?,
            whitelisted_tokens_for_aa: self
                .whitelisted_tokens_for_aa
                .iter()
                .enumerate()
                .map(|(i, k)| parse_h160(k).context(i))
                .collect::<Result<Vec<_>, _>>()
                .context("whitelisted_tokens_for_aa")?,
            extended_api_tracing: self.extended_api_tracing.unwrap_or_default(),
            api_namespaces,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            ws_port: Some(this.ws_port.into()),
            req_entities_limit: this.req_entities_limit,
            filters_disabled: Some(this.filters_disabled),
            mempool_cache_update_interval: this.mempool_cache_update_interval,
            mempool_cache_size: this.mempool_cache_size.map(|x| x.try_into().unwrap()),
            filters_limit: this.filters_limit,
            subscriptions_limit: this.subscriptions_limit,
            pubsub_polling_interval: this.pubsub_polling_interval,
            max_nonce_ahead: Some(this.max_nonce_ahead),
            gas_price_scale_factor: Some(this.gas_price_scale_factor),
            estimate_gas_scale_factor: Some(this.estimate_gas_scale_factor),
            estimate_gas_acceptable_overestimation: Some(
                this.estimate_gas_acceptable_overestimation,
            ),
            estimate_gas_optimize_search: Some(this.estimate_gas_optimize_search),
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
            latest_values_max_block_lag: this.latest_values_max_block_lag.map(NonZeroU32::get),
            fee_history_limit: this.fee_history_limit,
            max_batch_request_size: this.max_batch_request_size.map(|x| x.try_into().unwrap()),
            max_response_body_size_mb: this
                .max_response_body_size_mb
                .map(|x| x.try_into().unwrap()),
            max_response_body_size_overrides: this
                .max_response_body_size_overrides_mb
                .iter()
                .map(|(method, size_mb)| proto::MaxResponseSizeOverride {
                    method: Some(method.to_owned()),
                    size_mb: if size_mb == usize::MAX {
                        None
                    } else {
                        Some(size_mb.try_into().expect("failed converting usize to u64"))
                    },
                })
                .collect(),
            websocket_requests_per_minute_limit: this
                .websocket_requests_per_minute_limit
                .map(|x| x.into()),
            tree_api_url: this.tree_api_url.clone(),
            whitelisted_tokens_for_aa: this
                .whitelisted_tokens_for_aa
                .iter()
                .map(|k| format!("{:?}", k))
                .collect(),
            extended_api_tracing: Some(this.extended_api_tracing),
            api_namespaces: this.api_namespaces.clone().unwrap_or_default(),
        }
    }
}

impl ProtoRepr for proto::HealthCheck {
    type Type = api::HealthCheckConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            port: required(&self.port)
                .and_then(|&port| Ok(port.try_into()?))
                .context("port")?,
            slow_time_limit_ms: self.slow_time_limit_ms,
            hard_time_limit_ms: self.hard_time_limit_ms,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            port: Some(this.port.into()),
            slow_time_limit_ms: this.slow_time_limit_ms,
            hard_time_limit_ms: this.hard_time_limit_ms,
        }
    }
}

impl ProtoRepr for proto::MerkleTreeApi {
    type Type = api::MerkleTreeApiConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            port: required(&self.port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("port")?,
        })
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            port: Some(this.port.into()),
        }
    }
}
