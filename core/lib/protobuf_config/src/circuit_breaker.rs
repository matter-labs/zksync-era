use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::circuit_breaker as proto;

impl ProtoRepr for proto::CircuitBreaker {
    type Type = configs::chain::CircuitBreakerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            sync_interval_ms: *required(&self.sync_interval_ms).context("sync_interval_ms")?,
            http_req_max_retry_number: required(&self.http_req_max_retry_number)
                .and_then(|x| Ok((*x).try_into()?))
                .context("http_req_max_retry_number")?,
            http_req_retry_interval_sec: required(&self.http_req_retry_interval_sec)
                .and_then(|x| Ok((*x).try_into()?))
                .context("http_req_retry_interval_sec")?,
            replication_lag_limit_sec: self.replication_lag_limit_sec,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            sync_interval_ms: Some(this.sync_interval_ms),
            http_req_max_retry_number: Some(this.http_req_max_retry_number.try_into().unwrap()),
            http_req_retry_interval_sec: Some(this.http_req_retry_interval_sec.into()),
            replication_lag_limit_sec: this.replication_lag_limit_sec,
        }
    }
}
