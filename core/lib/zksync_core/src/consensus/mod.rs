//! Consensus-related functionality.
use std::sync::Arc;

use anyhow::Context as _;
use zksync_concurrency::{ctx, scope};
use zksync_consensus_executor::{Executor, Validator};
use zksync_consensus_roles::{node, validator};
use zksync_dal::ConnectionPool;
use zksync_protobuf::ProtoFmt;
use zksync_types::Address;

mod payload;
mod proto;
mod storage;

#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;

pub(crate) use self::{payload::Payload, storage::sync_block_to_consensus_block};

use serde::de::Error;
use std::collections::{HashMap, HashSet};
use zksync_consensus_crypto::{Text, TextFmt};

#[derive(PartialEq, Eq, Hash)]
pub struct SerdeText<T: TextFmt>(pub T);

impl<'de, T: TextFmt> serde::Deserialize<'de> for SerdeText<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self(
            T::decode(Text::new(<&str>::deserialize(d)?)).map_err(Error::custom)?,
        ))
    }
}

#[derive(serde::Deserialize)]
pub struct SerdeConfig {
    /// Local socket address to listen for the incoming connections.
    server_addr: std::net::SocketAddr,
    /// Public address of this node (should forward to server_addr)
    /// that will be advertised to peers, so that they can connect to this
    /// node.
    public_addr: std::net::SocketAddr,

    /// Validator private key. Should be set only for the validator node.
    validator_key: Option<SerdeText<validator::SecretKey>>,
    /// Validators participating in consensus.
    validator_set: Vec<SerdeText<validator::PublicKey>>,

    /// Key of this node. It uniquely identifies the node.
    pub node_key: SerdeText<node::SecretKey>,
    /// Limit on the number of inbound connections outside
    /// of the `static_inbound` set.
    pub gossip_dynamic_inbound_limit: u64,
    /// Inbound gossip connections that should be unconditionally accepted.
    pub gossip_static_inbound: HashSet<SerdeText<node::PublicKey>>,
    /// Outbound gossip connections that the node should actively try to
    /// establish and maintain.
    pub gossip_static_outbound: HashMap<SerdeText<node::PublicKey>, std::net::SocketAddr>,

    pub genesis_block_number: u32,
    pub operator_address: Option<Address>,
}

impl SerdeConfig {
    pub(crate) fn executor(&self) -> ExecutorConfig {
        ExecutorConfig {
            server_addr: self.server_addr.clone(),
            validators: self.validator_set.clone(),
            gossip: GossipConfig {
                key: cfg.node_key.public(),
                dynamic_inbound_limit: cfg.gossip_dynamic_inbound_limit,
                static_inbound: cfg.gossip_static_inbound.clone(),
                static_outbound: cfg.gossip_static_outbound.clone(),
            },
        }
    }
}

impl TryFrom<SerdeConfig> for Config {
    type Error = anyhow::Error;
    fn try_from(cfg: SerdeConfig) -> anyhow::Result<Self> {
        let validator_key = cfg.validator_key.context("validator_key is required")?;
        Ok(Self {
            executor: cfg.executor(),
            consensus: ConsensusConfig {
                key: validator_key.public(),
                public_addr: cfg.public_addr,
            },
            validator_key,
            node_key: cfg.node_key,
            operator_address: cfg
                .operator_address
                .context("operator_address is required")?,
        })
    }
}

#[derive(Debug)]
pub struct Config {
    pub executor: ExecutorConfig,
    pub consensus: ConsensusConfig,
    pub node_key: node::SecretKey,
    pub validator_key: validator::SecretKey,
    pub operator_address: Address,
}

impl Config {
    #[allow(dead_code)]
    pub async fn run(self, ctx: &ctx::Ctx, pool: ConnectionPool) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.executor.validators
                == validator::ValidatorSet::new(vec![self.validator_key.public()]).unwrap(),
            "currently only consensus with just 1 validator is supported"
        );
        let store = Arc::new(
            storage::SignedBlockStore::new(
                ctx,
                pool,
                &self.executor.genesis_block,
                self.operator_address,
            )
            .await?,
        );
        let mut executor = Executor::new(ctx, self.executor, self.node_key, store.clone()).await?;
        executor
            .set_validator(
                self.consensus,
                self.validator_key,
                store.clone(),
                store.clone(),
            )
            .context("executor.set_validator()")?;
        scope::run!(&ctx, |ctx, s| async {
            s.spawn_bg(store.run_background_tasks(ctx));
            executor.run(ctx).await
        })
        .await
    }
}
