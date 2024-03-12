use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto;

impl ProtoRepr for proto::FriProofCompressor {
    type Type = configs::FriProofCompressorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            compression_mode: required(&self.compression_mode)
                .and_then(|x| Ok((*x).try_into()?))
                .context("compression_mode")?,
            prometheus_listener_port: required(&self.prometheus_listener_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_listener_port")?,
            prometheus_pushgateway_url: required(&self.prometheus_pushgateway_url)
                .context("prometheus_pushgateway_url")?
                .clone(),
            prometheus_push_interval_ms: self.prometheus_push_interval_ms,
            generation_timeout_in_secs: required(&self.generation_timeout_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("generation_timeout_in_secs")?,
            max_attempts: *required(&self.max_attempts).context("max_attempts")?,
            universal_setup_path: required(&self.universal_setup_path)
                .context("universal_setup_path")?
                .clone(),
            universal_setup_download_url: required(&self.universal_setup_download_url)
                .context("universal_setup_download_url")?
                .clone(),
            verify_wrapper_proof: *required(&self.verify_wrapper_proof)
                .context("verify_wrapper_proof")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            compression_mode: Some(this.compression_mode.into()),
            prometheus_listener_port: Some(this.prometheus_listener_port.into()),
            prometheus_pushgateway_url: Some(this.prometheus_pushgateway_url.clone()),
            prometheus_push_interval_ms: this.prometheus_push_interval_ms,
            generation_timeout_in_secs: Some(this.generation_timeout_in_secs.into()),
            max_attempts: Some(this.max_attempts),
            universal_setup_path: Some(this.universal_setup_path.clone()),
            universal_setup_download_url: Some(this.universal_setup_download_url.clone()),
            verify_wrapper_proof: Some(this.verify_wrapper_proof),
        }
    }
}
