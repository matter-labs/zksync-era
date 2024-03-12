use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto;

impl proto::ProtocolVersionLoadingMode {
    fn new(x: &configs::proof_data_handler::ProtocolVersionLoadingMode) -> Self {
        type From = configs::proof_data_handler::ProtocolVersionLoadingMode;
        match x {
            From::FromDb => Self::FromDb,
            From::FromEnvVar => Self::FromEnvVar,
        }
    }
    fn parse(&self) -> configs::proof_data_handler::ProtocolVersionLoadingMode {
        type To = configs::proof_data_handler::ProtocolVersionLoadingMode;
        match self {
            Self::FromDb => To::FromDb,
            Self::FromEnvVar => To::FromEnvVar,
        }
    }
}

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
            protocol_version_loading_mode: required(&self.protocol_version_loading_mode)
                .and_then(|x| Ok(proto::ProtocolVersionLoadingMode::try_from(*x)?))
                .context("protocol_version_loading_mode")?
                .parse(),
            fri_protocol_version_id: required(&self.fri_protocol_version_id)
                .and_then(|x| Ok((*x).try_into()?))
                .context("fri_protocol_version_id")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            http_port: Some(this.http_port.into()),
            proof_generation_timeout_in_secs: Some(this.proof_generation_timeout_in_secs.into()),
            protocol_version_loading_mode: Some(
                proto::ProtocolVersionLoadingMode::new(&this.protocol_version_loading_mode).into(),
            ),
            fri_protocol_version_id: Some(this.fri_protocol_version_id.into()),
        }
    }
}
