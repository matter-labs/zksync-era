use anyhow::Context as _;
use zksync_config::configs::{self};
use zksync_protobuf::{required, ProtoRepr};

use crate::proto::observability as proto;

impl ProtoRepr for proto::Observability {
    type Type = configs::ObservabilityConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let (sentry_url, sentry_environment) = if let Some(sentry) = &self.sentry {
            let sentry_url = required(&sentry.url).context("sentry_url")?.clone();
            let sentry_url = if sentry_url.to_lowercase() == *"unset" {
                None
            } else {
                Some(sentry_url)
            };

            (
                sentry_url,
                Some(
                    required(&sentry.environment)
                        .context("sentry.environment")?
                        .clone(),
                ),
            )
        } else {
            (None, None)
        };
        Ok(Self::Type {
            sentry_url,
            sentry_environment,
            log_format: required(&self.log_format).context("log_format")?.clone(),
            opentelemetry: self
                .opentelemetry
                .as_ref()
                .map(|cfg| cfg.read().context("opentelemetry"))
                .transpose()?,
            log_directives: self.log_directives.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        let sentry = if this.sentry_url.is_none() || this.sentry_environment.is_none() {
            None
        } else {
            Some(proto::Sentry {
                url: this.sentry_url.clone(),
                environment: this.sentry_environment.clone(),
                panic_interval: None,
                error_interval: None,
            })
        };
        Self {
            sentry,
            log_format: Some(this.log_format.clone()),
            opentelemetry: this.opentelemetry.as_ref().map(ProtoRepr::build),
            log_directives: this.log_directives.clone(),
        }
    }
}

impl ProtoRepr for proto::Opentelemetry {
    type Type = configs::OpentelemetryConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            level: required(&self.level).context("level")?.clone(),
            endpoint: required(&self.endpoint).context("endpoint")?.clone(),
            logs_endpoint: self.logs_endpoint.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            level: Some(this.level.clone()),
            endpoint: Some(this.endpoint.clone()),
            logs_endpoint: this.logs_endpoint.clone(),
        }
    }
}
