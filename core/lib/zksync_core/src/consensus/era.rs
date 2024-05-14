//! This module provides convenience functions to run consensus components in different modes
//! as expected by the zkSync Era.
//!
//! This module simply glues APIs that are already publicly exposed by the `consensus` module,
//! so in case any custom behavior is needed, these APIs should be used directly.

use zksync_concurrency::ctx;
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};
use zksync_dal::Core;
use zksync_web3_decl::client::{DynClient, L2};

use super::{en, storage::ConnectionPool};
use crate::sync_layer::{sync_action::ActionQueueSender, SyncState};

/// Runs the consensus task in the main node mode.
pub async fn run_main_node(
    ctx: &ctx::Ctx,
    cfg: ConsensusConfig,
    secrets: ConsensusSecrets,
    pool: zksync_dal::ConnectionPool<Core>,
) -> anyhow::Result<()> {
    // Consensus is a new component.
    // For now in case of error we just log it and allow the server
    // to continue running.
    if let Err(err) = super::run_main_node(ctx, cfg, secrets, ConnectionPool(pool)).await {
        tracing::error!(%err, "Consensus actor failed");
    } else {
        tracing::info!("Consensus actor stopped");
    }
    Ok(())
}

/// Runs the consensus node for the external node.
/// If `cfg` is `None`, it will just fetch blocks from the main node
/// using JSON RPC, without starting the consensus node.
pub async fn run_en(
    ctx: &ctx::Ctx,
    cfg: Option<(ConsensusConfig, ConsensusSecrets)>,
    pool: zksync_dal::ConnectionPool<Core>,
    sync_state: SyncState,
    main_node_client: Box<DynClient<L2>>,
    actions: ActionQueueSender,
) -> anyhow::Result<()> {
    let en = en::EN {
        pool: ConnectionPool(pool),
        sync_state: sync_state.clone(),
        client: main_node_client,
    };
    let res = match cfg {
        Some((cfg, secrets)) => en.run(ctx, actions, cfg, secrets).await,
        None => en.run_fetcher(ctx, actions).await,
    };
    tracing::info!("Consensus actor stopped");
    res
}
