use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::required;

use crate::{proto, repr::ProtoRepr};

impl ProtoRepr for proto::EthClient {
    type Type = configs::ETHClientConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            chain_id: *required(&self.chain_id).context("chain_id")?,
            web3_url: required(&self.web3_url).context("web3_url")?.clone(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            chain_id: Some(this.chain_id),
            web3_url: Some(this.web3_url.clone()),
        }
    }
}
