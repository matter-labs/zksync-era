use anyhow::Context as _;
use zksync_basic_types::L2ChainId;
use zksync_config::configs::consensus::{
    AttesterPublicKey, ConsensusConfig, GenesisSpec, Host, NodePublicKey, ProtocolVersion,
    RpcConfig, ValidatorPublicKey, WeightedAttester, WeightedValidator,
};
use zksync_protobuf::{kB, read_optional, repr::ProtoRepr, required, ProtoFmt};

use crate::{proto::consensus as proto, read_optional_repr};

impl ProtoRepr for proto::WeightedValidator {
    type Type = WeightedValidator;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            key: ValidatorPublicKey(required(&self.key).context("key")?.clone()),
            weight: *required(&self.weight).context("weight")?,
        })
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            key: Some(this.key.0.clone()),
            weight: Some(this.weight),
        }
    }
}

impl ProtoRepr for proto::WeightedAttester {
    type Type = WeightedAttester;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            key: AttesterPublicKey(required(&self.key).context("key")?.clone()),
            weight: *required(&self.weight).context("weight")?,
        })
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            key: Some(this.key.0.clone()),
            weight: Some(this.weight),
        }
    }
}

impl ProtoRepr for proto::GenesisSpec {
    type Type = GenesisSpec;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            chain_id: required(&self.chain_id)
                .and_then(|x| L2ChainId::try_from(*x).map_err(|a| anyhow::anyhow!(a)))
                .context("chain_id")?,
            protocol_version: ProtocolVersion(
                *required(&self.protocol_version).context("protocol_version")?,
            ),
            validators: self
                .validators
                .iter()
                .enumerate()
                .map(|(i, x)| x.read().context(i))
                .collect::<Result<_, _>>()
                .context("validators")?,
            attesters: self
                .attesters
                .iter()
                .enumerate()
                .map(|(i, x)| x.read().context(i))
                .collect::<Result<_, _>>()
                .context("attesters")?,
            leader: ValidatorPublicKey(required(&self.leader).context("leader")?.clone()),
        })
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            chain_id: Some(this.chain_id.as_u64()),
            protocol_version: Some(this.protocol_version.0),
            validators: this.validators.iter().map(ProtoRepr::build).collect(),
            attesters: this.attesters.iter().map(ProtoRepr::build).collect(),
            leader: Some(this.leader.0.clone()),
        }
    }
}

impl ProtoRepr for proto::RpcConfig {
    type Type = RpcConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            get_block_rate: read_optional(&self.get_block_rate).context("get_block_rate")?,
        })
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            get_block_rate: this.get_block_rate.as_ref().map(ProtoFmt::build),
        }
    }
}

impl ProtoRepr for proto::Config {
    type Type = ConsensusConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let read_addr = |e: &proto::NodeAddr| {
            let key = NodePublicKey(required(&e.key).context("key")?.clone());
            let addr = Host(required(&e.addr).context("addr")?.clone());
            anyhow::Ok((key, addr))
        };

        let max_payload_size = required(&self.max_payload_size)
            .and_then(|x| Ok((*x).try_into()?))
            .context("max_payload_size")?;

        let max_batch_size = match self.max_batch_size {
            Some(x) => x.try_into().context("max_batch_size")?,
            None => {
                // Compute a default batch size: the batch interval is ~1 minute,
                // so there will be ~60 blocks, and an Ethereum Merkle proof is ~1kB.
                // Using 100 to be generous.
                max_payload_size * 100 + kB
            }
        };

        Ok(Self::Type {
            server_addr: required(&self.server_addr)
                .and_then(|x| Ok(x.parse()?))
                .context("server_addr")?,
            public_addr: Host(required(&self.public_addr).context("public_addr")?.clone()),
            max_payload_size,
            max_batch_size,
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
            genesis_spec: read_optional_repr(&self.genesis_spec).context("genesis_spec")?,
            rpc: read_optional_repr(&self.rpc_config).context("rpc_config")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            server_addr: Some(this.server_addr.to_string()),
            public_addr: Some(this.public_addr.0.clone()),
            max_payload_size: Some(this.max_payload_size.try_into().unwrap()),
            max_batch_size: Some(this.max_batch_size.try_into().unwrap()),
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
            genesis_spec: this.genesis_spec.as_ref().map(ProtoRepr::build),
            rpc_config: this.rpc.as_ref().map(ProtoRepr::build),
        }
    }
}
