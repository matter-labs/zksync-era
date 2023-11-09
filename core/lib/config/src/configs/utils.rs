use anyhow::Context as _;
use serde::Deserialize;
use std::{convert::TryInto as _, env, time::Duration};
use zksync_protobuf::{required, ProtoFmt};

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct PrometheusConfig {
    /// Port to which the Prometheus exporter server is listening.
    pub listener_port: u16,
    /// URL of the push gateway.
    pub pushgateway_url: String,
    /// Push interval in ms.
    pub push_interval_ms: Option<u64>,
}

impl PrometheusConfig {
    pub fn push_interval(&self) -> Duration {
        Duration::from_millis(self.push_interval_ms.unwrap_or(100))
    }

    /// Returns the full endpoint URL for the push gateway.
    pub fn gateway_endpoint(&self) -> String {
        let gateway_url = &self.pushgateway_url;
        let job_id = "zksync-pushgateway";
        let namespace =
            env::var("POD_NAMESPACE").unwrap_or_else(|_| "UNKNOWN_NAMESPACE".to_owned());
        let pod = env::var("POD_NAME").unwrap_or_else(|_| "UNKNOWN_POD".to_owned());
        format!("{gateway_url}/metrics/job/{job_id}/namespace/{namespace}/pod/{pod}")
    }
}

impl ProtoFmt for PrometheusConfig {
    type Proto = super::proto::Prometheus;
    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            listener_port: required(&r.listener_port)
                .and_then(|p| Ok((*p).try_into()?))
                .context("listener_port")?,
            pushgateway_url: required(&r.pushgateway_url)
                .context("pushgateway_url")?
                .clone(),
            push_interval_ms: r.push_interval_ms,
        })
    }

    fn build(&self) -> Self::Proto {
        Self::Proto {
            listener_port: Some(self.listener_port.into()),
            pushgateway_url: Some(self.pushgateway_url.clone()),
            push_interval_ms: self.push_interval_ms,
        }
    }
}
