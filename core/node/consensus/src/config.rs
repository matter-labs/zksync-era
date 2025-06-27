//! Configuration utilities for the consensus component.
use std::collections::{BTreeMap, HashMap};

use anyhow::Context as _;
use secrecy::{ExposeSecret as _, SecretString};
use zksync_concurrency::net;
use zksync_config::{
    configs,
    configs::consensus::{ConsensusConfig, ConsensusSecrets, Host, NodePublicKey},
};
use zksync_consensus_crypto::{Text, TextFmt};
use zksync_consensus_executor as executor;
use zksync_consensus_network as network;
use zksync_consensus_roles::{node, validator};
use zksync_dal::consensus_dal;
use zksync_types::ethabi;

fn read_secret_text<T: TextFmt>(text: Option<&SecretString>) -> anyhow::Result<Option<T>> {
    text.map(|text| Text::new(text.expose_secret()).decode())
        .transpose()
        .map_err(|_| anyhow::format_err!("invalid format"))
}

pub(super) fn validator_key(
    secrets: &ConsensusSecrets,
) -> anyhow::Result<Option<validator::SecretKey>> {
    read_secret_text(secrets.validator_key.as_ref())
}

/// Consensus genesis specification.
/// It is a digest of the `validator::Genesis`,
/// which allows to initialize genesis (if not present)
/// decide whether a hard fork is necessary (if present).
#[derive(Debug, PartialEq)]
pub(super) struct GenesisSpec {
    pub(super) chain_id: validator::ChainId,
    pub(super) protocol_version: validator::ProtocolVersion,
    pub(super) validators: Option<validator::Schedule>,
    pub(super) registry_address: Option<ethabi::Address>,
    pub(super) seed_peers: BTreeMap<node::PublicKey, net::Host>,
}

impl GenesisSpec {
    pub(super) fn from_global_config(cfg: &consensus_dal::GlobalConfig) -> Self {
        Self {
            chain_id: cfg.genesis.chain_id,
            protocol_version: cfg.genesis.protocol_version,
            validators: cfg.genesis.validators_schedule.clone(),
            registry_address: cfg.registry_address,
            seed_peers: cfg.seed_peers.clone(),
        }
    }

    pub(super) fn parse(x: &configs::consensus::GenesisSpec) -> anyhow::Result<Self> {
        let schedule = if x.validators.is_empty() || x.leader.is_none() {
            None
        } else {
            let leader = x.leader.as_ref().unwrap(); // safe to unwrap because of the check above

            let validators: Vec<_> = x
                .validators
                .iter()
                .enumerate()
                .map(|(i, (key, weight))| {
                    Ok(validator::ValidatorInfo {
                        key: Text::new(&key.0).decode().context("key").context(i)?,
                        weight: *weight,
                        leader: key == leader,
                    })
                })
                .collect::<anyhow::Result<_>>()
                .context("validators")?;

            Some(
                validator::Schedule::new(validators, validator::LeaderSelection::default())
                    .context("schedule")?,
            )
        };

        anyhow::ensure!(
            schedule.is_some() || x.registry_address.is_some(),
            "either validators or registry_address must be present"
        );

        Ok(Self {
            chain_id: validator::ChainId(x.chain_id.as_u64()),
            protocol_version: validator::ProtocolVersion(x.protocol_version.0),
            validators: schedule,
            registry_address: x.registry_address,
            seed_peers: x
                .seed_peers
                .iter()
                .map(|(key, addr)| {
                    anyhow::Ok((
                        Text::new(&key.0)
                            .decode::<node::PublicKey>()
                            .context("key")?,
                        net::Host(addr.0.clone()),
                    ))
                })
                .collect::<Result<_, _>>()
                .context("seed_peers")?,
        })
    }
}

pub(super) fn node_key(secrets: &ConsensusSecrets) -> anyhow::Result<Option<node::SecretKey>> {
    read_secret_text(secrets.node_key.as_ref())
}

pub(super) fn executor(
    cfg: &ConsensusConfig,
    global_config: &consensus_dal::GlobalConfig,
    build_version: Option<semver::Version>,
) -> anyhow::Result<executor::Config> {
    // Always connect to seed peers.
    // Once we implement dynamic peer discovery,
    // we won't establish a persistent connection to seed peers
    // but rather just ask them for more peers.
    let mut gossip_static_outbound: HashMap<_, _> = global_config
        .seed_peers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
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

    let debug_page = cfg
        .debug_page_addr
        .map(|addr| network::debug_page::Config { addr });

    Ok(executor::Config {
        build_version,
        server_addr: cfg.server_addr,
        public_addr: net::Host(cfg.public_addr.0.clone()),
        max_payload_size: cfg.max_payload_size.0 as usize,
        view_timeout: cfg.view_timeout.try_into().context("view_timeout")?,
        node_key: node_key(&cfg.secrets)
            .context("node_key")?
            .context("missing node_key")?,
        validator_key: validator_key(&cfg.secrets).context("validator_key")?,
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
        debug_page,
    })
}
