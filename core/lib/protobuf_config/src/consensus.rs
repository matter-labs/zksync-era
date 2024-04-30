use anyhow::Context as _;
use zksync_config::configs::consensus::{
    ConsensusConfig, ConsensusSecrets, Host, NodePublicKey, NodeSecretKey, ValidatorSecretKey,
};
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::consensus as proto;

impl ProtoRepr for proto::Config {
    type Type = ConsensusConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let read_addr = |e: &proto::NodeAddr| {
            let key = NodePublicKey(required(&e.key).context("key")?.clone());
            let addr = Host(required(&e.addr).context("addr")?.clone());
            anyhow::Ok((key, addr))
        };
        Ok(Self::Type {
            server_addr: required(&self.server_addr)
                .and_then(|x| Ok(x.parse()?))
                .context("server_addr")?,
            public_addr: Host(required(&self.public_addr).context("public_addr")?.clone()),
            max_payload_size: required(&self.max_payload_size)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_payload_size")?,
            gossip_dynamic_inbound_limit: required(&self.gossip_dynamic_inbound_limit)
                .and_then(|x| Ok((*x).try_into()?))
                .context("gossip_dynamic_inbound_limit")?,
            gossip_static_inbound: self
                .gossip_static_inbound
                .iter()
                .map(|x| NodePublicKey(x.clone()))
                .collect(),
            gossip_static_outbound: self
                .gossip_static_outbound
                .iter()
                .enumerate()
                .map(|(i, e)| read_addr(e).context(i))
                .collect::<Result<_, _>>()?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            server_addr: Some(this.server_addr.to_string()),
            public_addr: Some(this.public_addr.0.clone()),
            max_payload_size: Some(this.max_payload_size.try_into().unwrap()),
            gossip_dynamic_inbound_limit: Some(
                this.gossip_dynamic_inbound_limit.try_into().unwrap(),
            ),
            gossip_static_inbound: this
                .gossip_static_inbound
                .iter()
                .map(|x| x.0.clone())
                .collect(),
            gossip_static_outbound: this
                .gossip_static_outbound
                .iter()
                .map(|x| proto::NodeAddr {
                    key: Some(x.0 .0.clone()),
                    addr: Some(x.1 .0.clone()),
                })
                .collect(),
        }
    }
}

impl ProtoRepr for proto::Secrets {
    type Type = ConsensusSecrets;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            validator_key: self
                .validator_key
                .as_ref()
                .map(|x| ValidatorSecretKey(x.clone())),
            node_key: self.node_key.as_ref().map(|x| NodeSecretKey(x.clone())),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            validator_key: this.validator_key.as_ref().map(|x| x.0.clone()),
            node_key: this.node_key.as_ref().map(|x| x.0.clone()),
        }
    }
}
