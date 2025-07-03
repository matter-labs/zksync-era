//! Consensus-related functionality.

#![allow(clippy::redundant_locals)]
#![allow(clippy::needless_pass_by_ref_mut)]

use std::sync::Arc;

use anyhow::Context;
use zksync_concurrency::{ctx, error::Wrap, scope, time};
use zksync_config::configs::consensus::{ConsensusConfig, ConsensusSecrets};
use zksync_consensus_engine::EngineManager;
use zksync_consensus_executor::{self as executor};
use zksync_dal::{consensus_dal, Core};
use zksync_node_sync::sync_action::ActionQueueSender;
use zksync_state_keeper::StateKeeper;
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::EnNamespaceClient,
};

use crate::{
    registry::{Registry, RegistryAddress},
    storage::Store,
};

mod abi;
mod config;
mod metrics;
pub mod node;
mod registry;
mod storage;
// #[cfg(test)]
// pub(crate) mod testonly;
// #[cfg(test)]
// mod tests;
mod vm;

#[derive(Debug)]
pub enum RunMode {
    MainNode,
    NonMainNode {
        main_node_client: Box<DynClient<L2>>,
    },
}

#[allow(clippy::too_many_arguments)]
pub async fn run_node(
    ctx: &ctx::Ctx,
    cfg: ConsensusConfig,
    secrets: ConsensusSecrets,
    pool: zksync_dal::ConnectionPool<Core>,
    mode: RunMode,
    sk: StateKeeper,
    actions: ActionQueueSender,
    build_version: semver::Version,
) -> anyhow::Result<()> {
    let pool = storage::ConnectionPool(pool);
    tracing::info!(
        is_validator = secrets.validator_key.is_some(),
        is_main_node = matches!(mode, RunMode::MainNode),
        "running node"
    );
    let res: ctx::Result<()> = scope::run!(&ctx, |ctx, s| async {
        let (global_config, main_node_client) = match mode {
            RunMode::MainNode => {
                if let Some(spec) = &cfg.genesis_spec {
                    let spec = config::GenesisSpec::parse(spec).context("GenesisSpec::parse()")?;

                    pool.connection(ctx)
                        .await
                        .wrap("connection()")?
                        .adjust_global_config(ctx, &spec)
                        .await
                        .wrap("adjust_global_config()")?;
                }

                // Initialize global config.
                let config = pool
                    .connection(ctx)
                    .await
                    .wrap("connection()")?
                    .global_config(ctx)
                    .await
                    .wrap("global_config()")?
                    .context("global_config() disappeared")?;
                (config, None)
            }
            RunMode::NonMainNode { main_node_client } => {
                // Initialize global config.
                let global_config = fetch_global_config(main_node_client.as_ref(), ctx)
                    .await
                    .wrap("fetch_global_config()")?;

                let mut conn = pool.connection(ctx).await.wrap("connection()")?;
                conn.try_update_global_config(ctx, &global_config)
                    .await
                    .wrap("try_update_global_config()")?;

                // Monitor the genesis of the main node.
                // If it changes, it means that a hard fork occurred and we need to reset the consensus state.
                s.spawn_bg::<()>({
                    let old = global_config.clone();
                    let main_node_client = main_node_client.clone();
                    async {
                        let old = old;
                        let main_node_client = main_node_client;
                        loop {
                            if let Ok(new) =
                                fetch_global_config(main_node_client.as_ref(), ctx).await
                            {
                                // We verify the transition here to work around the situation
                                // where `consensus_global_config()` RPC fails randomly and fallback
                                // to `consensus_genesis()` RPC activates.
                                if new != old
                                    && consensus_dal::verify_config_transition(&old, &new).is_ok()
                                {
                                    return Err(anyhow::format_err!(
                                        "global config changed: old {old:?}, new {new:?}"
                                    )
                                    .into());
                                }
                            }
                            ctx.sleep(time::Duration::seconds(5)).await?;
                        }
                    }
                });

                (global_config, Some(main_node_client))
            }
        };

        // Initialize registry.
        let registry = Arc::new(match global_config.registry_address {
            Some(addr) => Some(Registry::new(pool.clone(), RegistryAddress::new(addr)).await),
            None => None,
        });

        let payload_queue = pool
            .connection(ctx)
            .await
            .wrap("connection()")?
            .new_payload_queue(actions)
            .await
            .wrap("new_payload_queue()")?;

        // The main node doesn't have a payload queue as it produces all the L2 blocks itself.
        let (store, runner) = Store::new(
            ctx,
            pool.clone(),
            payload_queue,
            main_node_client,
            registry.clone(),
            sk,
        )
        .await
        .wrap("Store::new()")?;
        s.spawn_bg(async { Ok(runner.run(ctx).await.context("Store::runner()")?) });

        let (engine_manager, engine_runner) = EngineManager::new(ctx, Box::new(store.clone()))
            .await
            .wrap("BlockStore::new()")?;
        s.spawn_bg(async { Ok(engine_runner.run(ctx).await.context("BlockStore::run()")?) });

        let executor = executor::Executor {
            config: config::executor(&cfg, &secrets, &global_config, Some(build_version))?,
            engine_manager,
        };

        tracing::info!("running the main node executor");
        executor.run(ctx).await.context("main node executor")?;
        Ok(())
    })
    .await;

    tracing::info!("Consensus actor stopped");
    match res {
        Ok(()) | Err(ctx::Error::Canceled(_)) => Ok(()),
        Err(ctx::Error::Internal(err)) => Err(err),
    }
}

#[tracing::instrument(skip_all)]
async fn fetch_global_config(
    client: &DynClient<L2>,
    ctx: &ctx::Ctx,
) -> ctx::Result<consensus_dal::GlobalConfig> {
    let cfg = ctx
        .wait(client.consensus_global_config())
        .await?
        .context("consensus_global_config()")?
        .context("main node is not running consensus component")?;
    Ok(zksync_protobuf::serde::Deserialize {
        deny_unknown_fields: false,
    }
    .proto_fmt(&cfg.0)
    .context("deserialize()")?)
}
