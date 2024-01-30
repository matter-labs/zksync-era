use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::required;

use crate::{proto, repr::ProtoRepr};

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
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            compilation_timeout: Some(this.compilation_timeout),
            polling_interval: this.polling_interval,
            prometheus_port: Some(this.prometheus_port.into()),
        }
    }
}
