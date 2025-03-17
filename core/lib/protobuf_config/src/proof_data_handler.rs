use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::prover as proto;

impl ProtoRepr for proto::ProofDataHandler {
    type Type = configs::ProofDataHandlerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            http_port: required(&self.http_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("http_port")?,
            proof_generation_timeout_in_secs: required(&self.proof_generation_timeout_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("proof_generation_timeout_in_secs")?,
            retry_connection_interval_in_secs: required(&self.retry_connection_interval_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("retry_connection_interval_in_secs")?,
            api_url: required(&self.api_url).context("api_url")?.clone(),
            batch_readiness_check_interval_in_secs: required(
                &self.batch_readiness_check_interval_in_secs,
            )
            .and_then(|x| Ok((*x).try_into()?))
            .context("api_poll_duration_in_secs")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            api_url: Some(this.api_url.clone()),
            batch_readiness_check_interval_in_secs: Some(
                this.batch_readiness_check_interval_in_secs.into(),
            ),
            retry_connection_interval_in_secs: Some(this.retry_connection_interval_in_secs.into()),
            proof_generation_timeout_in_secs: Some(this.proof_generation_timeout_in_secs.into()),
            // deprecated
            tee_support: None,
            // deprecated
            first_tee_processed_batch: None,
            // deprecated
            tee_proof_generation_timeout_in_secs: None,
            // deprecated
            tee_batch_permanently_ignored_timeout_in_hours: None,
        }
    }
}
