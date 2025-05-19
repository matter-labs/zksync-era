use anyhow::Context as _;
use zksync_basic_types::L2ChainId;
use zksync_concurrency::time;
use zksync_config::configs::consensus::{
    ConsensusConfig, GenesisSpec, Host, NodePublicKey, ProtocolVersion, RpcConfig,
    ValidatorPublicKey, WeightedValidator,
};
use zksync_protobuf::{kB, read_optional, repr::ProtoRepr, required, ProtoFmt};

use crate::{parse_h160, proto::consensus as proto, read_optional_repr};

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
            leader: ValidatorPublicKey(required(&self.leader).context("leader")?.clone()),
            registry_address: self
                .registry_address
                .as_ref()
                .map(|x| parse_h160(x))
                .transpose()
                .context("registry_address")?,
            seed_peers: self
                .seed_peers
                .iter()
                .enumerate()
                .map(|(i, e)| e.read().context(i))
                .collect::<Result<_, _>>()
                .context("seed_peers")?,
        })
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            chain_id: Some(this.chain_id.as_u64()),
            protocol_version: Some(this.protocol_version.0),
            validators: this.validators.iter().map(ProtoRepr::build).collect(),
            leader: Some(this.leader.0.clone()),
            registry_address: this.registry_address.map(|a| format!("{:?}", a)),
            seed_peers: this
                .seed_peers
                .iter()
                .map(|(k, v)| proto::NodeAddr::build(&(k.clone(), v.clone())))
                .collect(),
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

impl ProtoRepr for proto::NodeAddr {
    type Type = (NodePublicKey, Host);
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok((
            NodePublicKey(required(&self.key).context("key")?.clone()),
            Host(required(&self.addr).context("addr")?.clone()),
        ))
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            key: Some(this.0 .0.clone()),
            addr: Some(this.1 .0.clone()),
        }
    }
}

impl ProtoRepr for proto::Config {
    type Type = ConsensusConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let max_payload_size = required(&self.max_payload_size)
            .and_then(|x| Ok((*x).try_into()?))
            .context("max_payload_size")?;

        let max_batch_size = match self.max_batch_size {
            Some(x) => x.try_into().context("max_batch_size")?,
            None => {
                // Compute a default batch size, so operators are not caught out by the missing setting
                // while we're still working on batch syncing. The batch interval is ~1 minute,
                // so there will be ~60 blocks, and an Ethereum Merkle proof is ~1kB, but under high
                // traffic there can be thousands of huge transactions that quickly fill up blocks
                // and there could be more blocks in a batch then expected. We chose a generous
                // limit so as not to prevent any legitimate batch from being transmitted.
                max_payload_size * 5000 + kB
            }
        };

        Ok(Self::Type {
            port: self.port.and_then(|x| x.try_into().ok()),
            server_addr: required(&self.server_addr)
                .and_then(|x| Ok(x.parse()?))
                .context("server_addr")?,
            public_addr: Host(required(&self.public_addr).context("public_addr")?.clone()),
            max_payload_size,
            view_timeout: self
                .view_timeout
                .as_ref()
                .map(|x| time::Duration::read(x).context("view_timeout"))
                .transpose()?,
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
                .map(|(i, e)| e.read().context(i))
                .collect::<Result<_, _>>()
                .context("gossip_static_outbound")?,
            genesis_spec: read_optional_repr(&self.genesis_spec),
            rpc: read_optional_repr(&self.rpc_config),
            debug_page_addr: self
                .debug_page_addr
                .as_ref()
                .map(|x| Ok::<_, anyhow::Error>(x.parse()?))
                .transpose()
                .context("debug_page_addr")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            port: this.port.map(|x| x.into()),
            server_addr: Some(this.server_addr.to_string()),
            public_addr: Some(this.public_addr.0.clone()),
            max_payload_size: Some(this.max_payload_size.try_into().unwrap()),
            view_timeout: this.view_timeout.as_ref().map(ProtoFmt::build),
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
                .map(|(k, v)| proto::NodeAddr::build(&(k.clone(), v.clone())))
                .collect(),
            genesis_spec: this.genesis_spec.as_ref().map(ProtoRepr::build),
            rpc_config: this.rpc.as_ref().map(ProtoRepr::build),
            debug_page_addr: this.debug_page_addr.as_ref().map(|x| x.to_string()),
        }
    }
}
