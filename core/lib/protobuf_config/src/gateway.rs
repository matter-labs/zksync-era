use anyhow::Context as _;
use zksync_basic_types::SLChainId;
use zksync_config::configs::gateway::GatewayChainConfig;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::{parse_h160, proto::gateway as proto};

impl ProtoRepr for proto::GatewayChainConfig {
    type Type = GatewayChainConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            state_transition_proxy_addr: required(&self.state_transition_proxy_addr)
                .and_then(|x| parse_h160(x))
                .context("state_transition_proxy_addr")?,

            validator_timelock_addr: required(&self.validator_timelock_addr)
                .and_then(|x| parse_h160(x))
                .context("validator_timelock_addr")?,

            multicall3_addr: required(&self.multicall3_addr)
                .and_then(|x| parse_h160(x))
                .context("multicall3_addr")?,

            diamond_proxy_addr: required(&self.diamond_proxy_addr)
                .and_then(|x| parse_h160(x))
                .context("diamond_proxy_addr")?,

            chain_admin_addr: self
                .chain_admin_addr
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()?,

            governance_addr: required(&self.governance_addr)
                .and_then(|x| parse_h160(x))
                .context("governance_addr")?,
            gateway_chain_id: required(&self.gateway_chain_id)
                .map(|x| SLChainId(*x))
                .context("gateway_chain_id")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            state_transition_proxy_addr: Some(format!("{:?}", this.state_transition_proxy_addr)),
            validator_timelock_addr: Some(format!("{:?}", this.validator_timelock_addr)),
            multicall3_addr: Some(format!("{:?}", this.multicall3_addr)),
            diamond_proxy_addr: Some(format!("{:?}", this.diamond_proxy_addr)),
            chain_admin_addr: this.chain_admin_addr.map(|x| format!("{:?}", x)),
            governance_addr: Some(format!("{:?}", this.governance_addr)),
            gateway_chain_id: Some(this.gateway_chain_id.0),
        }
    }
}
