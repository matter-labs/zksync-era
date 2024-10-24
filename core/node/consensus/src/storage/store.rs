use std::sync::Arc;

use anyhow::Context as _;
use tracing::Instrument;
use zksync_concurrency::{ctx, error::Wrap as _, scope, sync, time};
use zksync_consensus_bft::PayloadManager;
use zksync_consensus_roles::validator;
use zksync_consensus_storage::{self as storage};
use zksync_dal::consensus_dal::{self, Payload};
use zksync_node_sync::fetcher::{FetchedBlock, FetchedTransaction};
use zksync_types::L2BlockNumber;
use zksync_web3_decl::{
    client::{DynClient, L2},
    namespaces::EnNamespaceClient as _,
};

use super::{Connection, PayloadQueue};
use crate::storage::{ConnectionPool, InsertCertificateError};

fn to_fetched_block(
    number: validator::BlockNumber,
    payload: &validator::Payload,
) -> anyhow::Result<FetchedBlock> {
    let number = L2BlockNumber(
        number
            .0
            .try_into()
            .context("Integer overflow converting block number")?,
    );
    let payload = Payload::decode(payload).context("Payload::decode()")?;
    Ok(FetchedBlock {
        number,
        l1_batch_number: payload.l1_batch_number,
        last_in_batch: payload.last_in_batch,
        protocol_version: payload.protocol_version,
        timestamp: payload.timestamp,
        reference_hash: Some(payload.hash),
        l1_gas_price: payload.l1_gas_price,
        l2_fair_gas_price: payload.l2_fair_gas_price,
        fair_pubdata_price: payload.fair_pubdata_price,
        pubdata_params: payload.pubdata_params,
        virtual_blocks: payload.virtual_blocks,
        operator_address: payload.operator_address,
        transactions: payload
            .transactions
            .into_iter()
            .map(FetchedTransaction::new)
            .collect(),
    })
}

/// Wrapper of `ConnectionPool` implementing `ReplicaStore`, `PayloadManager`,
/// `PersistentBlockStore`.
///
/// Contains queues to save Quorum Certificates received over gossip to the store
/// as and when the payload they are over becomes available.
#[derive(Clone, Debug)]
pub(crate) struct Store {
    pub(super) pool: ConnectionPool,
    /// Action queue to fetch/store L2 block payloads
    block_payloads: Arc<sync::Mutex<Option<PayloadQueue>>>,
    /// L2 block QCs received from consensus
    block_certificates: ctx::channel::UnboundedSender<validator::CommitQC>,
    /// Range of L2 blocks for which we have a QC persisted.
    blocks_persisted: sync::watch::Receiver<storage::BlockStoreState>,
    /// Main node client. None if this node is the main node.
    client: Option<Box<DynClient<L2>>>,
}

struct PersistedBlockState(sync::watch::Sender<storage::BlockStoreState>);

/// Background task of the `Store`.
pub struct StoreRunner {
    pool: ConnectionPool,
    blocks_persisted: PersistedBlockState,
    block_certificates: ctx::channel::UnboundedReceiver<validator::CommitQC>,
}

impl Store {
    pub(crate) async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        payload_queue: Option<PayloadQueue>,
        client: Option<Box<DynClient<L2>>>,
    ) -> ctx::Result<(Store, StoreRunner)> {
        let mut conn = pool.connection(ctx).await.wrap("connection()")?;

        // Initial state of persisted blocks
        let blocks_persisted = conn.block_store_state(ctx).await.wrap("blocks_range()")?;
        drop(conn);

        let blocks_persisted = sync::watch::channel(blocks_persisted).0;
        let (block_certs_send, block_certs_recv) = ctx::channel::unbounded();

        Ok((
            Store {
                pool: pool.clone(),
                block_certificates: block_certs_send,
                block_payloads: Arc::new(sync::Mutex::new(payload_queue)),
                blocks_persisted: blocks_persisted.subscribe(),
                client,
            },
            StoreRunner {
                pool,
                blocks_persisted: PersistedBlockState(blocks_persisted),
                block_certificates: block_certs_recv,
            },
        ))
    }

    /// Get a fresh connection from the pool.
    async fn conn(&self, ctx: &ctx::Ctx) -> ctx::Result<Connection> {
        self.pool.connection(ctx).await.wrap("connection")
    }

    /// Number of the next block to queue.
    pub(crate) async fn next_block(
        &self,
        ctx: &ctx::Ctx,
    ) -> ctx::OrCanceled<Option<validator::BlockNumber>> {
        Ok(sync::lock(ctx, &self.block_payloads)
            .await?
            .as_ref()
            .map(|p| p.next()))
    }

    /// Queues the next block.
    pub(crate) async fn queue_next_fetched_block(
        &self,
        ctx: &ctx::Ctx,
        block: FetchedBlock,
    ) -> ctx::Result<()> {
        let mut payloads = sync::lock(ctx, &self.block_payloads).await?.into_async();
        if let Some(payloads) = &mut *payloads {
            payloads.send(block).await.context("payloads.send()")?;
        }
        Ok(())
    }
}

impl PersistedBlockState {
    /// Updates `persisted` to new.
    /// Ends of the range can only be moved forward.
    /// If `persisted.first` is moved forward, it means that blocks have been pruned.
    /// If `persisted.last` is moved forward, it means that new blocks with certificates have been
    /// persisted.
    #[tracing::instrument(skip_all, fields(first = %new.first, next = ?new.next()))]
    fn update(&self, new: storage::BlockStoreState) {
        self.0.send_if_modified(|p| {
            if &new == p {
                return false;
            }
            p.first = p.first.max(new.first);
            if p.next() < new.next() {
                p.last = new.last;
            }
            true
        });
    }

    /// Checks if the given certificate should be eventually persisted.
    /// Only certificates block store state is a range of blocks for which we already have
    /// certificates and we need certs only for the later ones.
    fn should_be_persisted(&self, cert: &validator::CommitQC) -> bool {
        self.0.borrow().next() <= cert.header().number
    }

    /// Appends the `cert` to `persisted` range.
    #[tracing::instrument(skip_all, fields(batch_number = %cert.message.proposal.number))]
    fn advance(&self, cert: validator::CommitQC) {
        self.0.send_if_modified(|p| {
            if p.next() != cert.header().number {
                return false;
            }
            p.last = Some(storage::Last::Final(cert));
            true
        });
    }
}

impl StoreRunner {
    pub async fn run(self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let StoreRunner {
            pool,
            blocks_persisted,
            mut block_certificates,
        } = self;

        let res = scope::run!(ctx, |ctx, s| async {
            #[tracing::instrument(skip_all)]
            async fn update_blocks_persisted_iteration(
                ctx: &ctx::Ctx,
                pool: &ConnectionPool,
                blocks_persisted: &PersistedBlockState,
            ) -> ctx::Result<()> {
                const POLL_INTERVAL: time::Duration = time::Duration::seconds(1);

                let state = pool
                    .connection(ctx)
                    .await?
                    .block_store_state(ctx)
                    .await
                    .wrap("block_store_state()")?;
                blocks_persisted.update(state);
                ctx.sleep(POLL_INTERVAL).await?;

                Ok(())
            }

            s.spawn::<()>(async {
                // Loop updating `blocks_persisted` whenever blocks get pruned.
                loop {
                    update_blocks_persisted_iteration(ctx, &pool, &blocks_persisted).await?;
                }
            });

            #[tracing::instrument(skip_all)]
            async fn insert_block_certificates_iteration(
                ctx: &ctx::Ctx,
                pool: &ConnectionPool,
                block_certificates: &mut ctx::channel::UnboundedReceiver<validator::CommitQC>,
                blocks_persisted: &PersistedBlockState,
            ) -> ctx::Result<()> {
                const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);

                let cert = block_certificates
                    .recv(ctx)
                    .instrument(tracing::info_span!("wait_for_block_certificate"))
                    .await?;
                // Wait for the block to be persisted, so that we can attach a cert to it.
                // We may exit this loop without persisting the certificate in case the
                // corresponding block has been pruned in the meantime.
                while blocks_persisted.should_be_persisted(&cert) {
                    use consensus_dal::InsertCertificateError as E;
                    // Try to insert the cert.
                    let res = pool
                        .connection(ctx)
                        .await?
                        .insert_block_certificate(ctx, &cert)
                        .await;
                    match res {
                        Ok(()) => {
                            // Insertion succeeded: update persisted state
                            // and wait for the next cert.
                            blocks_persisted.advance(cert);
                            break;
                        }
                        Err(InsertCertificateError::Inner(E::MissingPayload)) => {
                            // the payload is not in storage, it's either not yet persisted
                            // or already pruned. We will retry after a delay.
                            ctx.sleep(POLL_INTERVAL)
                                .instrument(tracing::info_span!("wait_for_block"))
                                .await?;
                        }
                        Err(InsertCertificateError::Canceled(err)) => {
                            return Err(ctx::Error::Canceled(err))
                        }
                        Err(err) => Err(err).context("insert_block_certificate()")?,
                    }
                }

                Ok(())
            }

            // Loop inserting block certs to storage.
            loop {
                insert_block_certificates_iteration(
                    ctx,
                    &pool,
                    &mut block_certificates,
                    &blocks_persisted,
                )
                .await?;
            }
        })
        .await;

        match res {
            Err(ctx::Error::Canceled(_)) | Ok(()) => Ok(()),
            Err(ctx::Error::Internal(err)) => Err(err),
        }
    }
}

#[async_trait::async_trait]
impl storage::PersistentBlockStore for Store {
    async fn genesis(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::Genesis> {
        Ok(self
            .conn(ctx)
            .await?
            .global_config(ctx)
            .await?
            .context("not found")?
            .genesis)
    }

    fn persisted(&self) -> sync::watch::Receiver<storage::BlockStoreState> {
        self.blocks_persisted.clone()
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<validator::Block> {
        Ok(self
            .conn(ctx)
            .await?
            .block(ctx, number)
            .await?
            .context("not found")?)
    }

    async fn verify_pregenesis_block(
        &self,
        ctx: &ctx::Ctx,
        block: &validator::PreGenesisBlock,
    ) -> ctx::Result<()> {
        // We simply ask the main node for the payload hash and compare it against the received
        // payload.
        let meta = match &self.client {
            None => self
                .conn(ctx)
                .await?
                .block_metadata(ctx, block.number)
                .await?
                .context("metadata not in storage")?,
            Some(client) => {
                let meta = ctx
                    .wait(client.block_metadata(L2BlockNumber(
                        block.number.0.try_into().context("overflow")?,
                    )))
                    .await?
                    .context("block_metadata()")?
                    .context("metadata not available")?;
                zksync_protobuf::serde::Deserialize {
                    deny_unknown_fields: false,
                }
                .proto_fmt(&meta.0)
                .context("deserialize()")?
            }
        };
        if meta.payload_hash != block.payload.hash() {
            return Err(anyhow::format_err!("payload hash mismatch").into());
        }
        Ok(())
    }

    /// If actions queue is set (and the block has not been stored yet),
    /// the block will be translated into a sequence of actions.
    /// The received actions should be fed
    /// to `ExternalIO`, so that `StateKeeper` will store the corresponding L2 block in the db.
    ///
    /// `store_next_block()` call will wait synchronously for the L2 block.
    /// Once the L2 block is observed in storage, `store_next_block()` will store a cert for this
    /// L2 block.
    async fn queue_next_block(&self, ctx: &ctx::Ctx, block: validator::Block) -> ctx::Result<()> {
        let mut payloads = sync::lock(ctx, &self.block_payloads).await?.into_async();
        let (p, j) = match &block {
            validator::Block::Final(block) => (&block.payload, Some(&block.justification)),
            validator::Block::PreGenesis(block) => (&block.payload, None),
        };
        if let Some(payloads) = &mut *payloads {
            payloads
                .send(to_fetched_block(block.number(), p).context("to_fetched_block")?)
                .await
                .context("payloads.send()")?;
        }
        if let Some(justification) = j {
            self.block_certificates.send(justification.clone());
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl storage::ReplicaStore for Store {
    async fn state(&self, ctx: &ctx::Ctx) -> ctx::Result<storage::ReplicaState> {
        self.conn(ctx)
            .await?
            .replica_state(ctx)
            .await
            .wrap("replica_state()")
    }

    async fn set_state(&self, ctx: &ctx::Ctx, state: &storage::ReplicaState) -> ctx::Result<()> {
        self.conn(ctx)
            .await?
            .set_replica_state(ctx, state)
            .await
            .wrap("set_replica_state()")
    }
}

#[async_trait::async_trait]
impl PayloadManager for Store {
    /// Currently (for the main node) proposing is implemented as just converting an L2 block from db (without a cert) into a payload.
    async fn propose(
        &self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
    ) -> ctx::Result<validator::Payload> {
        const LARGE_PAYLOAD_SIZE: usize = 1 << 20;
        let payload = self
            .pool
            .wait_for_payload(ctx, block_number)
            .await
            .wrap("wait_for_payload")?;
        let encoded_payload = payload.encode();
        if encoded_payload.0.len() > LARGE_PAYLOAD_SIZE {
            tracing::warn!(
                "large payload ({}B) with {} transactions",
                encoded_payload.0.len(),
                payload.transactions.len()
            );
        }
        Ok(encoded_payload)
    }

    /// Verify that `payload` is a correct proposal for the block `block_number`.
    /// * for the main node it checks whether the same block is already present in storage.
    /// * for the EN validator
    ///   * if the block with this number was already applied, it checks that it was the
    ///     same block. It should always be true, because main node is the only proposer and
    ///     to propose a different block a hard fork is needed.
    ///   * otherwise, EN attempts to apply the received block. If the block was incorrect
    ///     the statekeeper is expected to crash the whole EN. Otherwise OK is returned.
    async fn verify(
        &self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
        payload: &validator::Payload,
    ) -> ctx::Result<()> {
        let mut payloads = sync::lock(ctx, &self.block_payloads).await?.into_async();
        if let Some(payloads) = &mut *payloads {
            let block = to_fetched_block(block_number, payload).context("to_fetched_block")?;
            let n = block.number;
            payloads.send(block).await.context("payload_queue.send()")?;
            // Wait for the block to be processed, without waiting for it to be stored.
            // TODO(BFT-459): this is not ideal, because we don't check here whether the
            // processed block is the same as `payload`. It will work correctly
            // with the current implementation of EN, but we should make it more
            // precise when block reverting support is implemented.
            ctx.wait(payloads.sync_state.wait_for_local_block(n))
                .await?;
        } else {
            let want = self.pool.wait_for_payload(ctx, block_number).await?;
            let got = Payload::decode(payload).context("Payload::decode(got)")?;
            if got != want {
                return Err(
                    anyhow::format_err!("unexpected payload: got {got:?} want {want:?}").into(),
                );
            }
        }
        Ok(())
    }
}
