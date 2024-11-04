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
            prometheus_port: required(&self.prometheus_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_port")?,
            port: required(&self.port)
                .and_then(|x| (*x).try_into().context("overflow"))
                .context("port")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            port: Some(this.port as u32),
            compilation_timeout: Some(this.compilation_timeout),
            prometheus_port: Some(this.prometheus_port.into()),
        }
    }
}
