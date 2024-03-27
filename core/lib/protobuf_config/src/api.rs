use anyhow::Context as _;
use zksync_config::configs::{api, ApiConfig};
use zksync_protobuf::{
    repr::{read_required_repr, ProtoRepr},
    required,
};

use crate::{parse_h256, proto::api as proto};

impl ProtoRepr for proto::Api {
    type Type = ApiConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            web3_json_rpc: read_required_repr(&self.web3_json_rpc).context("web3_json_rpc")?,
            contract_verification: read_required_repr(&self.contract_verification)
                .context("contract_verification")?,
            prometheus: read_required_repr(&self.prometheus).context("prometheus")?,
            healthcheck: read_required_repr(&self.healthcheck).context("healthcheck")?,
            merkle_tree: read_required_repr(&self.merkle_tree).context("merkle_tree")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            web3_json_rpc: Some(ProtoRepr::build(&this.web3_json_rpc)),
            contract_verification: Some(ProtoRepr::build(&this.contract_verification)),
            prometheus: Some(ProtoRepr::build(&this.prometheus)),
            healthcheck: Some(ProtoRepr::build(&this.healthcheck)),
            merkle_tree: Some(ProtoRepr::build(&this.merkle_tree)),
        }
    }
}

impl ProtoRepr for proto::Web3JsonRpc {
    type Type = api::Web3JsonRpcConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            http_port: required(&self.http_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("http_port")?,
            http_url: required(&self.http_url).context("http_url")?.clone(),
            ws_port: required(&self.ws_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("ws_port")?,
            ws_url: required(&self.ws_url).context("ws_url")?.clone(),
            req_entities_limit: self.req_entities_limit,
            filters_disabled: self.filters_disabled.unwrap_or(false),
            filters_limit: self.filters_limit,
            subscriptions_limit: self.subscriptions_limit,
            pubsub_polling_interval: self.pubsub_polling_interval,
            max_nonce_ahead: *required(&self.max_nonce_ahead).context("max_nonce_ahead")?,
            gas_price_scale_factor: *required(&self.gas_price_scale_factor)
                .context("gas_price_scale_factor")?,
            request_timeout: self.request_timeout,
            account_pks: self
                .account_pks
                .as_ref()
                .map(|keys| {
                    keys.keys
                        .iter()
                        .enumerate()
                        .map(|(i, k)| parse_h256(k).context(i))
                        .collect::<Result<_, _>>()
                        .context("keys")
                })
                .transpose()
                .context("account_pks")?,
            estimate_gas_scale_factor: *required(&self.estimate_gas_scale_factor)
                .context("estimate_gas_scale_factor")?,
            estimate_gas_acceptable_overestimation: *required(
                &self.estimate_gas_acceptable_overestimation,
            )
            .context("acceptable_overestimation")?,
            l1_to_l2_transactions_compatibility_mode: *required(
                &self.l1_to_l2_transactions_compatibility_mode,
            )
            .context("l1_to_l2_transactions_compatibility_mode")?,
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
                .context("latests_values_cache_size_mb")?,
            fee_history_limit: self.fee_history_limit,
            max_batch_request_size: self
                .max_batch_request_size
                .map(|x| x.try_into())
                .transpose()
                .context("max_batch_requres_size")?,
            max_response_body_size_mb: self
                .max_response_body_size_mb
                .map(|x| x.try_into())
                .transpose()
                .context("max_response_body_size_mb")?,
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
        })
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            http_url: Some(this.http_url.clone()),
            ws_port: Some(this.ws_port.into()),
            ws_url: Some(this.ws_url.clone()),
            req_entities_limit: this.req_entities_limit,
            filters_disabled: Some(this.filters_disabled),
            mempool_cache_update_interval: this.mempool_cache_update_interval,
            mempool_cache_size: this.mempool_cache_size.map(|x| x.try_into().unwrap()),
            filters_limit: this.filters_limit,
            subscriptions_limit: this.subscriptions_limit,
            pubsub_polling_interval: this.pubsub_polling_interval,
            max_nonce_ahead: Some(this.max_nonce_ahead),
            gas_price_scale_factor: Some(this.gas_price_scale_factor),
            request_timeout: this.request_timeout,
            account_pks: this.account_pks.as_ref().map(|keys| proto::PrivateKeys {
                keys: keys.iter().map(|k| k.as_bytes().into()).collect(),
            }),
            estimate_gas_scale_factor: Some(this.estimate_gas_scale_factor),
            estimate_gas_acceptable_overestimation: Some(
                this.estimate_gas_acceptable_overestimation,
            ),
            l1_to_l2_transactions_compatibility_mode: Some(
                this.l1_to_l2_transactions_compatibility_mode,
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
            fee_history_limit: this.fee_history_limit,
            max_batch_request_size: this.max_batch_request_size.map(|x| x.try_into().unwrap()),
            max_response_body_size_mb: this
                .max_response_body_size_mb
                .map(|x| x.try_into().unwrap()),
            websocket_requests_per_minute_limit: this
                .websocket_requests_per_minute_limit
                .map(|x| x.into()),
            tree_api_url: this.tree_api_url.clone(),
        }
    }
}

impl ProtoRepr for proto::ContractVerificationApi {
    type Type = api::ContractVerificationApiConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            port: required(&self.port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("port")?,
            url: required(&self.url).context("url")?.clone(),
        })
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            port: Some(this.port.into()),
            url: Some(this.url.clone()),
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
