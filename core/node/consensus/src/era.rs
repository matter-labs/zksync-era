//! This module provides convenience functions to run consensus components in different modes
//! as expected by the ZKsync Era.
//!
//! This module simply glues APIs that are already publicly exposed by the `consensus` module,
//! so in case any custom behavior is needed, these APIs should be used directly.

use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};
use zksync_dal::{consensus_dal, Core};
use zksync_node_sync::{sync_action::ActionQueueSender, SyncState};
use zksync_web3_decl::client::{DynClient, L2};

use super::{en, mn, storage::ConnectionPool};
use crate::registry;

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
    // Consensus is a new component.
    // For now in case of error we just log it and allow the server
    // to continue running.
    if let Err(err) = mn::run_main_node(ctx, cfg, secrets, ConnectionPool(pool)).await {
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
    cfg: Option<(ConsensusConfig, ConsensusSecrets)>,
    pool: zksync_dal::ConnectionPool<Core>,
    sync_state: SyncState,
    main_node_client: Box<DynClient<L2>>,
    actions: ActionQueueSender,
    build_version: semver::Version,
) -> anyhow::Result<()> {
    let en = en::EN {
        pool: ConnectionPool(pool),
        sync_state: sync_state.clone(),
        client: main_node_client.for_component("block_fetcher"),
    };
    let res = match cfg {
        Some((cfg, secrets)) => {
            tracing::info!(
                is_validator = secrets.validator_key.is_some(),
                "running external node"
            );
            en.run(ctx, actions, cfg, secrets, Some(build_version))
                .await
        }
        None => {
            tracing::info!("running fetcher");
            en.run_fetcher(ctx, actions).await
        }
    };
    tracing::info!("Consensus actor stopped");
    res
}

/// Periodically fetches the pending validator committee for the last certified block
/// and persists it in the database.
#[allow(dead_code)]
async fn validator_committee_monitor(
    ctx: &ctx::Ctx,
    pool: &ConnectionPool,
    cfg: consensus_dal::GlobalConfig,
) -> ctx::Result<()> {
    const POLL_INTERVAL: time::Duration = time::Duration::seconds(5);
    let registry = registry::Registry::new(pool.clone()).await;

    loop {
        // Get the last/current block number that was certified.
        let last_block_certificate_number = pool
            .connection(ctx)
            .await
            .wrap("connection()")?
            .last_block_certificate_number(ctx)
            .await
            .wrap("last_block_certificate_number()")?
            .ok_or_else(|| anyhow::anyhow!("no block certificates found"))?;

        // Get the pending validator committee for the last certified block. There might
        // not be a pending committee if there was no changes to the validator set. In this case
        // we don't do anything.
        let Some((committee, commit_block)) = registry
            .get_pending_validator_committee(
                ctx,
                cfg.registry_address.map(registry::Address::new),
                last_block_certificate_number,
            )
            .await
            .wrap("pending_validator_committee()")?
        else {
            tracing::info!("no pending validator committee");
            ctx.sleep(POLL_INTERVAL).await?;
            continue;
        };

        // Persist the pending committee.
        pool.connection(ctx)
            .await
            .wrap("connection")?
            .insert_validator_committee(ctx, last_block_certificate_number, &committee)
            .await
            .wrap("insert_validator_committee()")?;
        tracing::info!(
                "persisted pending validator committee at block {last_block_certificate_number:?} for activation at block {commit_block:?}"
            );

        ctx.sleep(POLL_INTERVAL).await?;
    }
}
