//! Consensus-related functionality.
use std::sync::Arc;

use anyhow::Context as _;
use zksync_concurrency::{ctx, scope};
use zksync_consensus_executor::{ConsensusConfig, Executor, ExecutorConfig};
use zksync_consensus_roles::{node, validator};
use zksync_dal::ConnectionPool;
use zksync_types::Address;

mod payload;
mod proto;
mod storage;

#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;

pub(crate) use self::{payload::Payload, storage::sync_block_to_consensus_block};

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
