use std::sync::Arc;

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, scope, time};
use zksync_consensus_engine::EngineManager;
use zksync_consensus_executor::{self as executor};
use zksync_dal::consensus_dal;
use zksync_node_sync::sync_action::ActionQueueSender;
use zksync_shared_resources::api::SyncState;
use zksync_types::L2BlockNumber;
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::{EnNamespaceClient as _, EthNamespaceClient as _},
};

use super::{config, storage::Store, ConsensusConfig, ConsensusSecrets};
use crate::{
    registry::{Registry, RegistryAddress},
    storage::ConnectionPool,
};

/// External node.
pub(super) struct EN {
    pub(super) pool: ConnectionPool,
    pub(super) sync_state: SyncState,
    pub(super) client: Box<DynClient<L2>>,
}

impl EN {
    /// Task running a consensus node for the external node.
    /// It may be a validator, but it cannot be a leader (cannot propose blocks).
    pub async fn run(
        self,
        ctx: &ctx::Ctx,
        actions: ActionQueueSender,
        cfg: ConsensusConfig,
        secrets: ConsensusSecrets,
        build_version: Option<semver::Version>,
    ) -> anyhow::Result<()> {
        tracing::info!("running the external node");

        let res: ctx::Result<()> = scope::run!(ctx, |ctx, s| async {
            // Update sync state in the background.
            s.spawn_bg(self.fetch_state_loop(ctx));

            // Initialize global config.
            let global_config = self
                .fetch_global_config(ctx)
                .await
                .wrap("fetch_global_config()")?;

            // Initialize registry.
            let registry = Arc::new(match global_config.registry_address {
                Some(addr) => {
                    Some(Registry::new(self.pool.clone(), RegistryAddress::new(addr)).await)
                }
                None => None,
            });

            let mut conn = self.pool.connection(ctx).await.wrap("connection()")?;
            conn.try_update_global_config(ctx, &global_config)
                .await
                .wrap("try_update_global_config()")?;

            let payload_queue = conn
                .new_payload_queue(ctx, actions, self.sync_state.clone())
                .await
                .wrap("new_payload_queue()")?;

            drop(conn);

            // Monitor the genesis of the main node.
            // If it changes, it means that a hard fork occurred and we need to reset the consensus state.
            s.spawn_bg::<()>({
                let old = global_config.clone();
                async {
                    let old = old;
                    loop {
                        if let Ok(new) = self.fetch_global_config(ctx).await {
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

            // Run consensus component.
            // External nodes have a payload queue which they use to fetch data from the main node.
            let (store, runner) = Store::new(
                ctx,
                self.pool.clone(),
                Some(payload_queue),
                Some(self.client.clone()),
                registry.clone(),
            )
            .await
            .wrap("Store::new()")?;
            s.spawn_bg(async { Ok(runner.run(ctx).await.context("Store::runner()")?) });

            let (engine_manager, engine_runner) = EngineManager::new(ctx, Box::new(store.clone()))
                .await
                .wrap("BlockStore::new()")?;
            s.spawn_bg(async { Ok(engine_runner.run(ctx).await.context("BlockStore::run()")?) });

            let executor = executor::Executor {
                config: config::executor(&cfg, &secrets, &global_config, build_version)?,
                engine_manager,
            };
            tracing::info!("running the external node executor");
            executor.run(ctx).await.context("external node executor")?;

            Ok(())
        })
        .await;

        match res {
            Ok(()) | Err(ctx::Error::Canceled(_)) => Ok(()),
            Err(ctx::Error::Internal(err)) => Err(err),
        }
    }

    /// Periodically fetches the head of the main node
    /// and updates `SyncState` accordingly.
    async fn fetch_state_loop(&self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        const DELAY_INTERVAL: time::Duration = time::Duration::milliseconds(500);
        const RETRY_INTERVAL: time::Duration = time::Duration::seconds(5);
        loop {
            match ctx.wait(self.client.get_block_number()).await? {
                Ok(head) => {
                    let head = L2BlockNumber(head.try_into().ok().context("overflow")?);
                    self.sync_state.set_main_node_block(head);
                    ctx.sleep(DELAY_INTERVAL).await?;
                }
                Err(err) => {
                    tracing::warn!("get_block_number(): {err}");
                    ctx.sleep(RETRY_INTERVAL).await?;
                }
            }
        }
    }

    /// Fetches consensus global configuration from the main node.
    #[tracing::instrument(skip_all)]
    async fn fetch_global_config(
        &self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<consensus_dal::GlobalConfig> {
        let cfg = ctx
            .wait(self.client.consensus_global_config())
            .await?
            .context("consensus_global_config()")?
            .context("main node is not running consensus component")?;
        Ok(zksync_protobuf::serde::Deserialize {
            deny_unknown_fields: false,
        }
        .proto_fmt(&cfg.0)
        .context("deserialize()")?)
    }
}
