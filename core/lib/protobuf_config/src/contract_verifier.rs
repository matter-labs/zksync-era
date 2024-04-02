use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::contract_verifier as proto;

impl ProtoRepr for proto::ContractVerifier {
    type Type = configs::ContractVerifierConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            compilation_timeout: *required(&self.compilation_timeout)
                .context("compilation_timeout")?,
            polling_interval: self.polling_interval,
            prometheus_port: required(&self.prometheus_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_port")?,
            url: required(&self.url).cloned().context("url")?,
            port: required(&self.port)
                .and_then(|x| (*x).try_into().context("overflow"))
                .context("port")?,
            threads_per_server: self
                .threads_per_server
                .map(|a| a.try_into())
                .transpose()
                .context("threads_per_server")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            port: Some(this.port as u32),
            url: Some(this.url.clone()),
            compilation_timeout: Some(this.compilation_timeout),
            polling_interval: this.polling_interval,
            threads_per_server: this.threads_per_server.map(|a| a as u32),
            prometheus_port: Some(this.prometheus_port.into()),
        }
    }
}
