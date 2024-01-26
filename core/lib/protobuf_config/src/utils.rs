use anyhow::Context as _;
use zksync_config::configs::PrometheusConfig;
use zksync_protobuf::required;

use crate::{proto, repr::ProtoRepr};

impl ProtoRepr for proto::Prometheus {
    type Type = PrometheusConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(PrometheusConfig {
            listener_port: required(&self.listener_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("listener_port")?,
            pushgateway_url: required(&self.pushgateway_url)
                .context("pushgateway_url")?
                .clone(),
            push_interval_ms: self.push_interval_ms,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            listener_port: Some(this.listener_port.into()),
            pushgateway_url: Some(this.pushgateway_url.clone()),
            push_interval_ms: this.push_interval_ms,
        }
    }
}
