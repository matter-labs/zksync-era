use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{required, ProtoRepr};

use crate::proto;

impl ProtoRepr for proto::Observability {
    type Type = configs::ObservabilityConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            sentry_url: self.sentry_url.clone(),
            sentry_environment: self.sentry_environment.clone(),
            log_format: required(&self.log_format).context("log_format")?.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            sentry_url: this.sentry_url.clone(),
            sentry_environment: this.sentry_environment.clone(),
            log_format: Some(this.log_format.clone()),
        }
    }
}
