use anyhow::Context as _;
use zksync_config::configs::{self};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::observability as proto;

impl ProtoRepr for proto::Observability {
    type Type = configs::ObservabilityConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            sentry_url: self.sentry_url.clone(),
            sentry_environment: self.sentry_environment.clone(),
            log_format: required(&self.log_format).context("log_format")?.clone(),
            opentelemetry: self
                .opentelemetry
                .as_ref()
                .map(|cfg| cfg.read().context("opentelemetry"))
                .transpose()?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            sentry_url: this.sentry_url.clone(),
            sentry_environment: this.sentry_environment.clone(),
            log_format: Some(this.log_format.clone()),
            opentelemetry: this.opentelemetry.as_ref().map(ProtoRepr::build),
        }
    }
}

impl ProtoRepr for proto::Opentelemetry {
    type Type = configs::OpentelemetryConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            level: required(&self.level).context("level")?.clone(),
            endpoint: required(&self.endpoint).context("endpoint")?.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            level: Some(this.level.clone()),
            endpoint: Some(this.endpoint.clone()),
        }
    }
}
