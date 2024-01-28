use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::required;

use crate::{proto, repr::ProtoRepr};

impl ProtoRepr for proto::FriProverGateway {
    type Type = configs::FriProverGatewayConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            api_url: required(&self.api_url).context("api_url")?.clone(),
            api_poll_duration_secs: required(&self.api_poll_duration_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("api_poll_duration_secs")?,
            prometheus_listener_port: required(&self.prometheus_listener_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_listener_port")?,
            prometheus_pushgateway_url: required(&self.prometheus_pushgateway_url)
                .context("prometheus_pushgateway_url")?
                .clone(),
            prometheus_push_interval_ms: self.prometheus_push_interval_ms,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            api_url: Some(this.api_url.clone()),
            api_poll_duration_secs: Some(this.api_poll_duration_secs.into()),
            prometheus_listener_port: Some(this.prometheus_listener_port.into()),
            prometheus_pushgateway_url: Some(this.prometheus_pushgateway_url.clone()),
            prometheus_push_interval_ms: this.prometheus_push_interval_ms,
        }
    }
}
