use std::sync::Arc;

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, scope, time};
use zksync_consensus_executor::{self as executor, attestation};
use zksync_consensus_roles::{attester, validator};
use zksync_consensus_storage::{BatchStore, BlockStore};
use zksync_dal::consensus_dal;
use zksync_node_sync::{
    fetcher::FetchedBlock, sync_action::ActionQueueSender, MainNodeClient, SyncState,
};
use zksync_protobuf::ProtoFmt as _;
use zksync_types::L2BlockNumber;
use zksync_web3_decl::client::{DynClient, L2};

use super::{config, storage::Store, ConsensusConfig, ConsensusSecrets};
use crate::storage::{self, ConnectionPool};
use crate::registry;

/// External node.
pub(super) struct EN {
    pub(super) pool: ConnectionPool,
    pub(super) sync_state: SyncState,
    pub(super) client: Box<DynClient<L2>>,
}

impl EN {
    /// Task running a consensus node for the external node.
    /// It may be a validator, but it cannot be a leader (cannot propose blocks).
    ///
    /// NOTE: Before starting the consensus node it fetches all the blocks
    /// older than consensus genesis from the main node using json RPC.
    pub async fn run(
        self,
        ctx: &ctx::Ctx,
        actions: ActionQueueSender,
        cfg: ConsensusConfig,
        secrets: ConsensusSecrets,
    ) -> anyhow::Result<()> {
        let attester = config::attester_key(&secrets).context("attester_key")?;

        tracing::debug!(
            is_attester = attester.is_some(),
            "external node attester mode"
        );

        let res: ctx::Result<()> = scope::run!(ctx, |ctx, s| async {
            // Update sync state in the background.
            s.spawn_bg(self.fetch_state_loop(ctx));

            // Initialize genesis.
            let genesis = self.fetch_genesis(ctx).await.wrap("fetch_genesis()")?;
            let mut conn = self.pool.connection(ctx).await.wrap("connection()")?;

            conn.try_update_genesis(ctx, &genesis)
                .await
                .wrap("set_genesis()")?;

            let mut payload_queue = conn
                .new_payload_queue(ctx, actions, self.sync_state.clone())
                .await
                .wrap("new_payload_queue()")?;

            drop(conn);

            // Fetch blocks before the genesis.
            self.fetch_blocks(ctx, &mut payload_queue, Some(genesis.first_block))
                .await
                .wrap("fetch_blocks()")?;

            // Monitor the genesis of the main node.
            // If it changes, it means that a hard fork occurred and we need to reset the consensus state.
            s.spawn_bg::<()>({
                let old = genesis.clone();
                async {
                    let old = old;
                    loop {
                        if let Ok(new) = self.fetch_genesis(ctx).await {
                            if new != old {
                                return Err(anyhow::format_err!(
                                    "genesis changed: old {old:?}, new {new:?}"
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
            let (store, runner) = Store::new(ctx, self.pool.clone(), Some(payload_queue))
                .await
                .wrap("Store::new()")?;
            s.spawn_bg(async { Ok(runner.run(ctx).await?) });

            let (block_store, runner) = BlockStore::new(ctx, Box::new(store.clone()))
                .await
                .wrap("BlockStore::new()")?;
            s.spawn_bg(async { Ok(runner.run(ctx).await?) });

            let (batch_store, runner) = BatchStore::new(ctx, Box::new(store.clone()))
                .await
                .wrap("BatchStore::new()")?;
            s.spawn_bg(async { Ok(runner.run(ctx).await?) });

            let attestation = Arc::new(attestation::Controller::new(attester));
            s.spawn_bg(self.run_attestation_controller(ctx, genesis.clone(), attestation.clone()));

            let executor = executor::Executor {
                config: config::executor(&cfg, &secrets)?,
                block_store,
                batch_store,
                validator: config::validator_key(&secrets)
                    .context("validator_key")?
                    .map(|key| executor::Validator {
                        key,
                        replica_store: Box::new(store.clone()),
                        payload_manager: Box::new(store.clone()),
                    }),
                attestation,
            };
            tracing::info!("running the external node executor");
            executor.run(ctx).await?;

            Ok(())
        })
        .await;
        match res {
            Ok(()) | Err(ctx::Error::Canceled(_)) => Ok(()),
            Err(ctx::Error::Internal(err)) => Err(err),
        }
    }

    /// Task fetching L2 blocks using JSON-RPC endpoint of the main node.
    pub async fn run_fetcher(
        self,
        ctx: &ctx::Ctx,
        actions: ActionQueueSender,
    ) -> anyhow::Result<()> {
        tracing::warn!("\
            WARNING: this node is using ZKsync API synchronization, which will be deprecated soon. \
            Please follow this instruction to switch to p2p synchronization: \
            https://github.com/matter-labs/zksync-era/blob/main/docs/guides/external-node/09_decentralization.md");
        let res: ctx::Result<()> = scope::run!(ctx, |ctx, s| async {
            // Update sync state in the background.
            s.spawn_bg(self.fetch_state_loop(ctx));
            let mut payload_queue = self
                .pool
                .connection(ctx)
                .await
                .wrap("connection()")?
                .new_payload_queue(ctx, actions, self.sync_state.clone())
                .await
                .wrap("new_fetcher_cursor()")?;
            self.fetch_blocks(ctx, &mut payload_queue, None).await
        })
        .await;
        match res {
            Ok(()) | Err(ctx::Error::Canceled(_)) => Ok(()),
            Err(ctx::Error::Internal(err)) => Err(err),
        }
    }

    /// Monitors the `AttestationStatus` on the main node,
    /// and updates the attestation config accordingly.
    async fn run_attestation_controller(
        &self,
        ctx: &ctx::Ctx,
        genesis: validator::Genesis,
        attestation: Arc<attestation::Controller>,
    ) -> ctx::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::seconds(5);
        let registry = registry::Registry::new(genesis.clone(), self.pool.clone()).await;
        let mut next = attester::BatchNumber(0);
        loop {
            let status = loop {
                match self.fetch_attestation_status(ctx).await {
                    Err(err) => tracing::warn!("{err:#}"),
                    Ok(status) => {
                        if status.genesis != genesis.hash() {
                            return Err(anyhow::format_err!("genesis mismatch").into());
                        }
                        if status.next_batch_to_attest >= next {
                            break status;
                        }
                    }
                }
                ctx.sleep(POLL_INTERVAL).await?;
            };
            next = status.next_batch_to_attest.next();
            tracing::info!(
                "waiting for hash of batch {:?}",
                status.next_batch_to_attest
            );
            let hash = self
                .pool
                .wait_for_batch_hash(ctx, status.next_batch_to_attest)
                .await?;
            let Some(committee) = registry.attester_committee_for(ctx,status.consensus_registry_address, status.next_batch_to_attest).await.wrap("attester_committee_for()")? else {
                tracing::info!("attestation not required");
                continue;
            };
            let committee = Arc::new(committee);
            tracing::info!(
                "attesting batch {:?} with hash {hash:?}",
                status.next_batch_to_attest
            );
            attestation
                .start_attestation(Arc::new(attestation::Info {
                    batch_to_attest: attester::Batch {
                        genesis: status.genesis,
                        hash,
                        number: status.next_batch_to_attest,
                    },
                    committee: committee.clone(),
                }))
                .await
                .context("start_attestation()")?;
        }
    }

    /// Periodically fetches the head of the main node
    /// and updates `SyncState` accordingly.
    async fn fetch_state_loop(&self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        const DELAY_INTERVAL: time::Duration = time::Duration::milliseconds(500);
        const RETRY_INTERVAL: time::Duration = time::Duration::seconds(5);
        loop {
            match ctx.wait(self.client.fetch_l2_block_number()).await? {
                Ok(head) => {
                    self.sync_state.set_main_node_block(head);
                    ctx.sleep(DELAY_INTERVAL).await?;
                }
                Err(err) => {
                    tracing::warn!("main_node_client.fetch_l2_block_number(): {err}");
                    ctx.sleep(RETRY_INTERVAL).await?;
                }
            }
        }
    }

    /// Fetches genesis from the main node.
    #[tracing::instrument(skip_all)]
    async fn fetch_genesis(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::Genesis> {
        let genesis = ctx
            .wait(self.client.fetch_consensus_genesis())
            .await?
            .context("fetch_consensus_genesis()")?
            .context("main node is not running consensus component")?;
        // Deserialize the json, but don't allow for unknown fields.
        // We need to compute the hash of the Genesis, so simply ignoring the unknown fields won't
        // do.
        Ok(validator::GenesisRaw::read(
            &zksync_protobuf::serde::deserialize_proto_with_options(
                &genesis.0, /*deny_unknown_fields=*/ true,
            )
            .context("deserialize")?,
        )?
        .with_hash())
    }

    #[tracing::instrument(skip_all)]
    async fn fetch_attestation_status(
        &self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<consensus_dal::AttestationStatus> {
        match ctx.wait(self.client.fetch_attestation_status()).await? {
            Ok(Some(status)) => Ok(zksync_protobuf::serde::deserialize(&status.0)
                .context("deserialize(AttestationStatus")?),
            Ok(None) => Err(anyhow::format_err!("empty response").into()),
            Err(err) => Err(anyhow::format_err!(
                "AttestationStatus call to main node HTTP RPC failed: {err:#}"
            )
            .into()),
        }
    }

    /// Fetches (with retries) the given block from the main node.
    async fn fetch_block(&self, ctx: &ctx::Ctx, n: L2BlockNumber) -> ctx::Result<FetchedBlock> {
        const RETRY_INTERVAL: time::Duration = time::Duration::seconds(5);

        loop {
            let res = ctx.wait(self.client.fetch_l2_block(n, true)).await?;
            match res {
                Ok(Some(block)) => return Ok(block.try_into()?),
                Ok(None) => {}
                Err(err) if err.is_retriable() => {}
                Err(err) => {
                    return Err(anyhow::format_err!("client.fetch_l2_block({}): {err}", n).into());
                }
            }
            ctx.sleep(RETRY_INTERVAL).await?;
        }
    }

    /// Fetches blocks from the main node in range `[cursor.next()..end)`.
    pub(super) async fn fetch_blocks(
        &self,
        ctx: &ctx::Ctx,
        queue: &mut storage::PayloadQueue,
        end: Option<validator::BlockNumber>,
    ) -> ctx::Result<()> {
        const MAX_CONCURRENT_REQUESTS: usize = 30;
        let first = queue.next();
        let mut next = first;
        scope::run!(ctx, |ctx, s| async {
            let (send, mut recv) = ctx::channel::bounded(MAX_CONCURRENT_REQUESTS);
            s.spawn(async {
                let send = send;
                while end.map_or(true, |end| next < end) {
                    let n = L2BlockNumber(next.0.try_into().unwrap());
                    self.sync_state.wait_for_main_node_block(ctx, n).await?;
                    send.send(ctx, s.spawn(self.fetch_block(ctx, n))).await?;
                    next = next.next();
                }
                Ok(())
            });
            while end.map_or(true, |end| queue.next() < end) {
                let block = recv.recv(ctx).await?.join(ctx).await?;
                queue.send(block).await?;
            }
            Ok(())
        })
        .await?;
        // If fetched anything, wait for the last block to be stored persistently.
        if first < queue.next() {
            self.pool
                .wait_for_payload(ctx, queue.next().prev().unwrap())
                .await?;
        }
        Ok(())
    }
}
