//! This module provides convenience functions to run consensus components in different modes
//! as expected by the zkSync Era.
//!
//! This module simply glues APIs that are already publicly exposed by the `consensus` module,
//! so in case any custom behavior is needed, these APIs should be used directly.

use std::sync::Arc;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_concurrency::{ctx, limiter, scope, time};
use zksync_dal::{ConnectionPool, Core};

use super::{
    config::{Config, Secrets},
    fetcher::Fetcher,
    storage::Store,
};
use crate::sync_layer::{sync_action::ActionQueueSender, MainNodeClient, SyncState};

/// Runs the consensus task in the main node mode.
/// Uses provided context to run the tasks, but respects the stop signal.
pub async fn run_main_node(
    ctx: &ctx::Ctx,
    cfg: super::MainNodeConfig,
    pool: ConnectionPool<Core>,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    scope::run!(ctx, |ctx, s| async {
        s.spawn_bg(async {
            // Consensus is a new component.
            // For now in case of error we just log it and allow the server
            // to continue running.
            if let Err(err) = cfg.run(ctx, Store(pool)).await {
                tracing::error!(%err, "Consensus actor failed");
            } else {
                tracing::info!("Consensus actor stopped");
            }
            Ok(())
        });
        let _ = stop_receiver.wait_for(|stop| *stop).await?;
        Ok(())
    })
    .await
}

/// Runs the consensus in the fetcher mode (e.g. for the external node needs).
/// The fetcher implementation may either be p2p or centralized.
/// Uses provided context to run the tasks, but respects the stop signal.
pub async fn run_fetcher(
    ctx: &ctx::Ctx,
    cfg: Option<(Config, Secrets)>,
    pool: ConnectionPool<Core>,
    sync_state: SyncState,
    main_node_client: Arc<dyn MainNodeClient>,
    action_queue_sender: ActionQueueSender,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let fetcher = Fetcher {
        store: Store(pool),
        sync_state: sync_state.clone(),
        client: main_node_client,
        limiter: limiter::Limiter::new(
            ctx,
            limiter::Rate {
                burst: 10,
                refresh: time::Duration::milliseconds(30),
            },
        ),
    };
    let actions = action_queue_sender;
    scope::run!(&ctx, |ctx, s| async {
        s.spawn_bg(async {
            let res = match cfg {
                Some((cfg, secrets)) => fetcher.run_p2p(ctx, actions, cfg.p2p(&secrets)?).await,
                None => fetcher.run_centralized(ctx, actions).await,
            };
            tracing::info!("Consensus actor stopped");
            res
        });
        ctx.wait(stop_receiver.wait_for(|stop| *stop)).await??;
        Ok(())
    })
    .await
    .context("consensus actor")
}
