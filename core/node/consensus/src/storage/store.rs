use std::sync::Arc;

use anyhow::Context as _;
use tokio::sync::watch::Sender;
use tracing::Instrument;
use zksync_concurrency::{ctx, error::Wrap as _, scope, sync, time};
use zksync_consensus_bft::PayloadManager;
use zksync_consensus_roles::{attester, attester::BatchNumber, validator};
use zksync_consensus_storage::{self as storage, BatchStoreState};
use zksync_dal::consensus_dal::{self, Payload};
use zksync_node_sync::fetcher::{FetchedBlock, FetchedTransaction};
use zksync_types::L2BlockNumber;

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
/// `PersistentBlockStore` and `PersistentBatchStore`.
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
    /// L1 batch QCs received from consensus
    batch_certificates: ctx::channel::UnboundedSender<attester::BatchQC>,
    /// Range of L2 blocks for which we have a QC persisted.
    blocks_persisted: sync::watch::Receiver<storage::BlockStoreState>,
    /// Range of L1 batches we have persisted.
    batches_persisted: sync::watch::Receiver<storage::BatchStoreState>,
}

struct PersistedBlockState(sync::watch::Sender<storage::BlockStoreState>);

/// Background task of the `Store`.
pub struct StoreRunner {
    pool: ConnectionPool,
    blocks_persisted: PersistedBlockState,
    batches_persisted: sync::watch::Sender<storage::BatchStoreState>,
    block_certificates: ctx::channel::UnboundedReceiver<validator::CommitQC>,
    batch_certificates: ctx::channel::UnboundedReceiver<attester::BatchQC>,
}

impl Store {
    pub(crate) async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        payload_queue: Option<PayloadQueue>,
    ) -> ctx::Result<(Store, StoreRunner)> {
        let mut conn = pool.connection(ctx).await.wrap("connection()")?;

        // Initial state of persisted blocks
        let blocks_persisted = conn
            .block_certificates_range(ctx)
            .await
            .wrap("block_certificates_range()")?;

        // Initial state of persisted batches
        let batches_persisted = conn.batches_range(ctx).await.wrap("batches_range()")?;

        drop(conn);

        let blocks_persisted = sync::watch::channel(blocks_persisted).0;
        let batches_persisted = sync::watch::channel(batches_persisted).0;
        let (block_certs_send, block_certs_recv) = ctx::channel::unbounded();
        let (batch_certs_send, batch_certs_recv) = ctx::channel::unbounded();

        Ok((
            Store {
                pool: pool.clone(),
                block_certificates: block_certs_send,
                batch_certificates: batch_certs_send,
                block_payloads: Arc::new(sync::Mutex::new(payload_queue)),
                blocks_persisted: blocks_persisted.subscribe(),
                batches_persisted: batches_persisted.subscribe(),
            },
            StoreRunner {
                pool,
                blocks_persisted: PersistedBlockState(blocks_persisted),
                batches_persisted,
                block_certificates: block_certs_recv,
                batch_certificates: batch_certs_recv,
            },
        ))
    }

    /// Get a fresh connection from the pool.
    async fn conn(&self, ctx: &ctx::Ctx) -> ctx::Result<Connection> {
        self.pool.connection(ctx).await.wrap("connection")
    }
}

impl PersistedBlockState {
    /// Updates `persisted` to new.
    /// Ends of the range can only be moved forward.
    /// If `persisted.first` is moved forward, it means that blocks have been pruned.
    /// If `persisted.last` is moved forward, it means that new blocks with certificates have been
    /// persisted.
    #[tracing::instrument(skip_all, fields(first = %new.first, last = ?new.last.as_ref().map(|l| l.message.proposal.number)))]
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

    /// Checks if the given certificate is exactly the next one that should
    /// be persisted.
    fn should_be_persisted(&self, cert: &validator::CommitQC) -> bool {
        self.0.borrow().next() == cert.header().number
    }

    /// Appends the `cert` to `persisted` range.
    #[tracing::instrument(skip_all, fields(batch_number = %cert.message.proposal.number))]
    fn advance(&self, cert: validator::CommitQC) {
        self.0.send_if_modified(|p| {
            if p.next() != cert.header().number {
                return false;
            }
            p.last = Some(cert);
            true
        });
    }
}

impl StoreRunner {
    pub async fn run(self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let StoreRunner {
            pool,
            blocks_persisted,
            batches_persisted,
            mut block_certificates,
            mut batch_certificates,
        } = self;

        let res = scope::run!(ctx, |ctx, s| async {
            #[tracing::instrument(skip_all)]
            async fn update_blocks_persisted_iteration(
                ctx: &ctx::Ctx,
                pool: &ConnectionPool,
                blocks_persisted: &PersistedBlockState,
            ) -> ctx::Result<()> {
                const POLL_INTERVAL: time::Duration = time::Duration::seconds(1);

                let range = pool
                    .connection(ctx)
                    .await?
                    .block_certificates_range(ctx)
                    .await
                    .wrap("block_certificates_range()")?;
                blocks_persisted.update(range);
                ctx.sleep(POLL_INTERVAL).await?;

                Ok(())
            }

            s.spawn::<()>(async {
                // Loop updating `blocks_persisted` whenever blocks get pruned.
                loop {
                    update_blocks_persisted_iteration(ctx, &pool, &blocks_persisted).await?;
                }
            });

            #[tracing::instrument(skip_all, fields(l1_batch = %next_batch_number))]
            async fn gossip_sync_batches_iteration(
                ctx: &ctx::Ctx,
                pool: &ConnectionPool,
                next_batch_number: &mut BatchNumber,
                batches_persisted: &Sender<BatchStoreState>,
            ) -> ctx::Result<()> {
                const POLL_INTERVAL: time::Duration = time::Duration::seconds(1);

                let mut conn = pool.connection(ctx).await?;
                if let Some(last_batch_number) = conn
                    .get_last_batch_number(ctx)
                    .await
                    .wrap("last_batch_number()")?
                {
                    if last_batch_number >= *next_batch_number {
                        let range = conn.batches_range(ctx).await.wrap("batches_range()")?;
                        *next_batch_number = last_batch_number.next();
                        tracing::info_span!("batches_persisted_send").in_scope(|| {
                            batches_persisted.send_replace(range);
                        });
                    }
                }
                ctx.sleep(POLL_INTERVAL).await?;

                Ok(())
            }

            // NOTE: Running this update loop will trigger the gossip of `SyncBatches` which is currently
            // pointless as there is no proof and we have to ignore them. We can disable it, but bear in
            // mind that any node which gossips the availability will cause pushes and pulls in the consensus.
            s.spawn::<()>(async {
                // Loop updating `batches_persisted` whenever a new L1 batch is available in the database.
                // We have to do this because the L1 batch is produced as L2 blocks are executed,
                // which can happen on a different machine or in a different process, so we can't rely on some
                // DAL method updating this memory construct. However I'm not sure that `BatchStoreState`
                // really has to contain the full blown last batch, or whether it could have for example
                // just the number of it. We can't just use the `attester::BatchQC`, which would make it
                // analogous to the `BlockStoreState`, because the `SyncBatch` mechanism is for catching
                // up with L1 batches from peers _without_ the QC, based on L1 inclusion proofs instead.
                // Nevertheless since the `SyncBatch` contains all transactions for all L2 blocks,
                // we can try to make it less frequent by querying just the last batch number first.
                let mut next_batch_number = { batches_persisted.borrow().next() };
                loop {
                    gossip_sync_batches_iteration(
                        ctx,
                        &pool,
                        &mut next_batch_number,
                        &batches_persisted,
                    )
                    .await?;
                }
            });

            #[tracing::instrument(skip_all)]
            async fn insert_batch_certificates_iteration(
                ctx: &ctx::Ctx,
                pool: &ConnectionPool,
                batch_certificates: &mut ctx::channel::UnboundedReceiver<attester::BatchQC>,
            ) -> ctx::Result<()> {
                const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);

                let cert = batch_certificates
                    .recv(ctx)
                    .instrument(tracing::info_span!("wait_for_batch_certificate"))
                    .await?;

                loop {
                    use consensus_dal::InsertCertificateError as E;
                    // Try to insert the cert.
                    let res = pool
                        .connection(ctx)
                        .await?
                        .insert_batch_certificate(ctx, &cert)
                        .await;

                    match res {
                        Ok(()) => {
                            break;
                        }
                        Err(InsertCertificateError::Inner(E::MissingPayload)) => {
                            // The L1 batch isn't available yet.
                            // We can wait until it's produced/received, or we could modify gossip
                            // so that we don't even accept votes until we have the corresponding batch.
                            ctx.sleep(POLL_INTERVAL)
                                .instrument(tracing::info_span!("wait_for_batch"))
                                .await?;
                        }
                        Err(InsertCertificateError::Inner(err)) => {
                            return Err(ctx::Error::Internal(anyhow::Error::from(err)))
                        }
                        Err(InsertCertificateError::Canceled(err)) => {
                            return Err(ctx::Error::Canceled(err))
                        }
                    }
                }

                Ok(())
            }

            s.spawn::<()>(async {
                // Loop inserting batch certificates into storage
                loop {
                    insert_batch_certificates_iteration(ctx, &pool, &mut batch_certificates)
                        .await?;
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
                        Err(InsertCertificateError::Inner(err)) => {
                            return Err(ctx::Error::Internal(anyhow::Error::from(err)))
                        }
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
            .genesis(ctx)
            .await?
            .context("not found")?)
    }

    fn persisted(&self) -> sync::watch::Receiver<storage::BlockStoreState> {
        self.blocks_persisted.clone()
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<validator::FinalBlock> {
        Ok(self
            .conn(ctx)
            .await?
            .block(ctx, number)
            .await?
            .context("not found")?)
    }

    /// If actions queue is set (and the block has not been stored yet),
    /// the block will be translated into a sequence of actions.
    /// The received actions should be fed
    /// to `ExternalIO`, so that `StateKeeper` will store the corresponding L2 block in the db.
    ///
    /// `store_next_block()` call will wait synchronously for the L2 block.
    /// Once the L2 block is observed in storage, `store_next_block()` will store a cert for this
    /// L2 block.
    async fn queue_next_block(
        &self,
        ctx: &ctx::Ctx,
        block: validator::FinalBlock,
    ) -> ctx::Result<()> {
        let mut payloads = sync::lock(ctx, &self.block_payloads).await?.into_async();
        if let Some(payloads) = &mut *payloads {
            payloads
                .send(to_fetched_block(block.number(), &block.payload).context("to_fetched_block")?)
                .await
                .context("payload_queue.send()")?;
        }
        self.block_certificates.send(block.justification);
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

#[async_trait::async_trait]
impl storage::PersistentBatchStore for Store {
    /// Range of batches persisted in storage.
    fn persisted(&self) -> sync::watch::Receiver<BatchStoreState> {
        self.batches_persisted.clone()
    }

    /// Get the earliest L1 batch number which has to be signed by attesters.
    async fn earliest_batch_number_to_sign(
        &self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<attester::BatchNumber>> {
        // This is the rough roadmap of how this logic will evolve:
        // 1. Make best effort at gossiping and collecting votes; the `BatchVotes` in consensus only considers the last vote per attesters.
        //    Still, we can re-sign more than the last batch, anticipating step 2.
        // 2. Ask the Main Node what is the earliest batch number that it still expects votes for (ie. what is the last submission + 1).
        // 3. Change `BatchVotes` to handle multiple pending batch numbers, anticipating that batch intervals might decrease dramatically.
        // 4. Once QC is required to submit to L1, Look at L1 to figure out what is the last submission, and sign after that.

        // Originally this method returned all unsigned batch numbers by doing a DAL query, but we decided it should be okay and cheap
        // to resend signatures for already signed batches, and we don't have to worry about skipping them. Because of that, we also
        // didn't think it makes sense to query the database for the earliest unsigned batch *after* the submission, because we might
        // as well just re-sign everything. Until we have a way to argue about the "last submission" we just re-sign the last 10 to
        // try to produce as many QCs as the voting register allows, within reason.

        // The latest decision is not to store batches with gaps between in the database *of the main node*.
        // Once we have an API to serve to external nodes the earliest number the main node wants them to sign,
        // we can get rid of this method: on the main node we can sign from what `last_batch_qc` returns, and
        // while external nodes we can go from whatever the API returned.

        const NUM_BATCHES_TO_SIGN: u64 = 10;

        let Some(last_batch_number) = self
            .conn(ctx)
            .await?
            .get_last_batch_number(ctx)
            .await
            .wrap("get_last_batch_number")?
        else {
            return Ok(None);
        };

        Ok(Some(attester::BatchNumber(
            last_batch_number.0.saturating_sub(NUM_BATCHES_TO_SIGN),
        )))
    }

    /// Get the L1 batch QC from storage with the highest number.
    ///
    /// This might have gaps before it. Until there is a way to catch up with missing
    /// certificates by fetching from the main node, returning the last inserted one
    /// is the best we can do.
    async fn last_batch_qc(&self, ctx: &ctx::Ctx) -> ctx::Result<Option<attester::BatchQC>> {
        let Some(number) = self
            .conn(ctx)
            .await?
            .get_last_batch_certificate_number(ctx)
            .await
            .wrap("get_last_batch_certificate_number")?
        else {
            return Ok(None);
        };

        self.get_batch_qc(ctx, number).await
    }

    /// Returns the batch with the given number.
    async fn get_batch(
        &self,
        ctx: &ctx::Ctx,
        number: attester::BatchNumber,
    ) -> ctx::Result<Option<attester::SyncBatch>> {
        self.conn(ctx)
            .await?
            .get_batch(ctx, number)
            .await
            .wrap("get_batch")
    }

    /// Returns the [attester::Batch] with the given number, which is the `message` that
    /// appears in [attester::BatchQC], and represents the content that needs to be signed
    /// by the attesters.
    async fn get_batch_to_sign(
        &self,
        ctx: &ctx::Ctx,
        number: attester::BatchNumber,
    ) -> ctx::Result<Option<attester::Batch>> {
        let Some(hash) = self
            .conn(ctx)
            .await?
            .batch_hash(ctx, number)
            .await
            .wrap("batch_hash()")?
        else {
            return Ok(None);
        };
        Ok(Some(attester::Batch { number, hash }))
    }

    /// Returns the QC of the batch with the given number.
    async fn get_batch_qc(
        &self,
        ctx: &ctx::Ctx,
        number: attester::BatchNumber,
    ) -> ctx::Result<Option<attester::BatchQC>> {
        self.conn(ctx)
            .await?
            .batch_certificate(ctx, number)
            .await
            .wrap("batch_certificate")
    }

    /// Store the given QC in the storage.
    ///
    /// Storing a QC is allowed even if it creates a gap in the L1 batch history.
    /// If we need the last batch QC that still needs to be signed then the queries need to look for gaps.
    async fn store_qc(&self, _ctx: &ctx::Ctx, qc: attester::BatchQC) -> ctx::Result<()> {
        // Storing asynchronously because we might get the QC before the L1 batch itself.
        self.batch_certificates.send(qc);
        Ok(())
    }

    /// Queue the batch to be persisted in storage.
    ///
    /// The caller [BatchStore] ensures that this is only called when the batch is the next expected one.
    async fn queue_next_batch(
        &self,
        _ctx: &ctx::Ctx,
        _batch: attester::SyncBatch,
    ) -> ctx::Result<()> {
        // Currently the gossiping of `SyncBatch` and the `BatchStoreState` is unconditionally started by the `Network::run_stream` in consensus,
        // and as long as any node reports new batches available by updating the `PersistentBatchStore::persisted` here, the other nodes
        // will start pulling the corresponding batches, which will end up being passed to this method.
        // If we return an error here or panic, it will stop the whole consensus task tree due to the way scopes work, so instead just return immediately.
        // In the future we have to validate the proof agains the L1 state root hash, which IIUC we can't do just yet.

        // Err(anyhow::format_err!("unimplemented: queue_next_batch should not be called until we have the stateless L1 batch story completed.").into())

        Ok(())
    }
}
