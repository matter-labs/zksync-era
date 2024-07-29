//! Configuration utilities for the consensus component.
use std::collections::HashMap;

use anyhow::Context as _;
use secrecy::{ExposeSecret as _, Secret};
use zksync_concurrency::{limiter, net, time};
use zksync_config::{
    configs,
    configs::consensus::{ConsensusConfig, ConsensusSecrets, Host, NodePublicKey},
};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_executor as executor;
use zksync_consensus_roles::{attester, node, validator};

fn read_secret_text<T: TextFmt>(text: Option<&Secret<String>>) -> anyhow::Result<Option<T>> {
    text.map(|text| Text::new(text.expose_secret()).decode())
        .transpose()
        .map_err(|_| anyhow::format_err!("invalid format"))
}

pub(super) fn validator_key(
    secrets: &ConsensusSecrets,
) -> anyhow::Result<Option<validator::SecretKey>> {
    read_secret_text(secrets.validator_key.as_ref().map(|x| &x.0))
}

pub(super) fn attester_key(
    secrets: &ConsensusSecrets,
) -> anyhow::Result<Option<attester::SecretKey>> {
    read_secret_text(secrets.attester_key.as_ref().map(|x| &x.0))
}

/// Consensus genesis specification.
/// It is a digest of the `validator::Genesis`,
/// which allows to initialize genesis (if not present)
/// decide whether a hard fork is necessary (if present).
#[derive(Debug, PartialEq)]
pub(super) struct GenesisSpec {
    pub(super) chain_id: validator::ChainId,
    pub(super) protocol_version: validator::ProtocolVersion,
    pub(super) validators: validator::Committee,
    pub(super) attesters: Option<attester::Committee>,
    pub(super) leader_selection: validator::LeaderSelectionMode,
}

impl GenesisSpec {
    pub(super) fn from_genesis(g: &validator::Genesis) -> Self {
        Self {
            chain_id: g.chain_id,
            protocol_version: g.protocol_version,
            validators: g.validators.clone(),
            attesters: g.attesters.clone(),
            leader_selection: g.leader_selection.clone(),
        }
    }

    pub(super) fn parse(x: &configs::consensus::GenesisSpec) -> anyhow::Result<Self> {
        let validators: Vec<_> = x
            .validators
            .iter()
            .enumerate()
            .map(|(i, v)| {
                Ok(validator::WeightedValidator {
                    key: Text::new(&v.key.0).decode().context("key").context(i)?,
                    weight: v.weight,
                })
            })
            .collect::<anyhow::Result<_>>()
            .context("validators")?;

        let attesters: Vec<_> = x
            .attesters
            .iter()
            .enumerate()
            .map(|(i, v)| {
                Ok(attester::WeightedAttester {
                    key: Text::new(&v.key.0).decode().context("key").context(i)?,
                    weight: v.weight,
                })
            })
            .collect::<anyhow::Result<_>>()
            .context("attesters")?;

        Ok(Self {
            chain_id: validator::ChainId(x.chain_id.as_u64()),
            protocol_version: validator::ProtocolVersion(x.protocol_version.0),
            leader_selection: validator::LeaderSelectionMode::Sticky(
                Text::new(&x.leader.0).decode().context("leader")?,
            ),
            validators: validator::Committee::new(validators).context("validators")?,
            attesters: if attesters.is_empty() {
                None
            } else {
                Some(attester::Committee::new(attesters).context("attesters")?)
            },
        })
    }
}

pub(super) fn node_key(secrets: &ConsensusSecrets) -> anyhow::Result<Option<node::SecretKey>> {
    read_secret_text(secrets.node_key.as_ref().map(|x| &x.0))
}

pub(super) fn executor(
    cfg: &ConsensusConfig,
    secrets: &ConsensusSecrets,
) -> anyhow::Result<executor::Config> {
    let mut gossip_static_outbound = HashMap::new();
    {
        let mut append = |key: &NodePublicKey, addr: &Host| {
            gossip_static_outbound.insert(
                Text::new(&key.0).decode().context("key")?,
                net::Host(addr.0.clone()),
            );
            anyhow::Ok(())
        };
        for (i, (k, v)) in cfg.gossip_static_outbound.iter().enumerate() {
            append(k, v).with_context(|| format!("gossip_static_outbound[{i}]"))?;
        }
    }

    let mut rpc = executor::RpcConfig::default();
    rpc.get_block_rate = cfg.rpc().get_block_rate();
    // Disable batch syncing, because it is not implemented.
    rpc.get_batch_rate = limiter::Rate {
        burst: 0,
        refresh: time::Duration::ZERO,
    };

    Ok(executor::Config {
        server_addr: cfg.server_addr,
        public_addr: net::Host(cfg.public_addr.0.clone()),
        max_payload_size: cfg.max_payload_size,
        max_batch_size: cfg.max_batch_size,
        node_key: node_key(secrets)
            .context("node_key")?
            .context("missing node_key")?,
        gossip_dynamic_inbound_limit: cfg.gossip_dynamic_inbound_limit,
        gossip_static_inbound: cfg
            .gossip_static_inbound
            .iter()
            .enumerate()
            .map(|(i, x)| Text::new(&x.0).decode().context(i))
            .collect::<Result<_, _>>()
            .context("gossip_static_inbound")?,
        gossip_static_outbound,
        rpc,
        // TODO: Add to configuration
        debug_page: None,
    })
}
