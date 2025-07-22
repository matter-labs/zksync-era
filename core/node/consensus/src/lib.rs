//! Consensus-related functionality.

#![allow(clippy::redundant_locals)]
#![allow(clippy::needless_pass_by_ref_mut)]

use zksync_concurrency::ctx;
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};
use zksync_dal::Core;
use zksync_node_sync::sync_action::ActionQueueSender;
use zksync_shared_resources::api::SyncState;
use zksync_web3_decl::client::{DynClient, L2};

mod abi;
mod config;
mod en;
mod metrics;
mod mn;
pub mod node;
mod registry;
mod storage;
#[cfg(test)]
pub(crate) mod testonly;
#[cfg(test)]
mod tests;
mod vm;

/// Runs the consensus task in the main node mode.
pub async fn run_main_node(
    ctx: &ctx::Ctx,
    cfg: ConsensusConfig,
    secrets: ConsensusSecrets,
    pool: zksync_dal::ConnectionPool<Core>,
) -> anyhow::Result<()> {
    tracing::info!(
        is_validator = secrets.validator_key.is_some(),
        "running main node"
    );

    // For now in case of error we just log it and allow the server
    // to continue running.
    if let Err(err) = mn::run_main_node(ctx, cfg, secrets, storage::ConnectionPool(pool)).await {
        tracing::error!("Consensus actor failed: {err:#}");
    } else {
        tracing::info!("Consensus actor stopped");
    }
    Ok(())
}

/// Runs the consensus node for the external node.
/// If `cfg` is `None`, it will just fetch blocks from the main node
/// using JSON RPC, without starting the consensus node.
pub async fn run_external_node(
    ctx: &ctx::Ctx,
    config: (ConsensusConfig, ConsensusSecrets),
    pool: zksync_dal::ConnectionPool<Core>,
    sync_state: SyncState,
    main_node_client: Box<DynClient<L2>>,
    actions: ActionQueueSender,
    build_version: semver::Version,
) -> anyhow::Result<()> {
    let (cfg, secrets) = config;
    let en = en::EN {
        pool: storage::ConnectionPool(pool),
        sync_state: sync_state.clone(),
        client: main_node_client.for_component("block_fetcher"),
    };
    tracing::info!(
        is_validator = secrets.validator_key.is_some(),
        "running external node"
    );
    let res = en
        .run(ctx, actions, cfg, secrets, Some(build_version))
        .await;
    tracing::info!("Consensus actor stopped");
    res
}
