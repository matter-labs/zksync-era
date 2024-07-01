use std::sync::Arc;

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, scope, sync, time};
use zksync_consensus_bft::PayloadManager;
use zksync_consensus_roles::{attester, validator};
use zksync_consensus_storage::{self as storage, BatchStoreState};
use zksync_dal::consensus_dal::{self, Payload};
use zksync_node_sync::fetcher::{FetchedBlock, FetchedTransaction};
use zksync_types::L2BlockNumber;

use super::PayloadQueue;
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
    /// L2 block QCs received over gossip
    block_certificates: ctx::channel::UnboundedSender<validator::CommitQC>,
    /// Range of L2 blocks for which we have a QC persisted.
    blocks_persisted: sync::watch::Receiver<storage::BlockStoreState>,
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
    ) -> ctx::Result<(Store, StoreRunner)> {
        let blocks_persisted = pool
            .connection(ctx)
            .await
            .wrap("connection()")?
            .block_certificates_range(ctx)
            .await
            .wrap("certificates_range()")?;
        let blocks_persisted = sync::watch::channel(blocks_persisted).0;
        let (certs_send, certs_recv) = ctx::channel::unbounded();
        Ok((
            Store {
                pool: pool.clone(),
                block_certificates: certs_send,
                block_payloads: Arc::new(sync::Mutex::new(payload_queue)),
                blocks_persisted: blocks_persisted.subscribe(),
            },
            StoreRunner {
                pool,
                blocks_persisted: PersistedBlockState(blocks_persisted),
                block_certificates: certs_recv,
            },
        ))
    }
}

impl PersistedBlockState {
    /// Updates `persisted` to new.
    /// Ends of the range can only be moved forward.
    /// If `persisted.first` is moved forward, it means that blocks have been pruned.
    /// If `persisted.last` is moved forward, it means that new blocks with certificates have been
    /// persisted.
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
    pub async fn run(mut self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let res = scope::run!(ctx, |ctx, s| async {
            s.spawn::<()>(async {
                // Loop updating `persisted` whenever blocks get pruned.
                const POLL_INTERVAL: time::Duration = time::Duration::seconds(1);
                loop {
                    let range = self
                        .pool
                        .connection(ctx)
                        .await
                        .wrap("connection")?
                        .block_certificates_range(ctx)
                        .await
                        .wrap("certificates_range()")?;
                    self.blocks_persisted.update(range);
                    ctx.sleep(POLL_INTERVAL).await?;
                }
            });

            // Loop inserting certs to storage.
            const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
            loop {
                let cert = self.block_certificates.recv(ctx).await?;
                // Wait for the block to be persisted, so that we can attach a cert to it.
                // We may exit this loop without persisting the certificate in case the
                // corresponding block has been pruned in the meantime.
                while self.blocks_persisted.should_be_persisted(&cert) {
                    use consensus_dal::InsertCertificateError as E;
                    // Try to insert the cert.
                    let res = self
                        .pool
                        .connection(ctx)
                        .await
                        .wrap("connection")?
                        .insert_block_certificate(ctx, &cert)
                        .await;
                    match res {
                        Ok(()) => {
                            // Insertion succeeded: update persisted state
                            // and wait for the next cert.
                            self.blocks_persisted.advance(cert);
                            break;
                        }
                        Err(InsertCertificateError::Inner(E::MissingPayload)) => {
                            // the payload is not in storage, it's either not yet persisted
                            // or already pruned. We will retry after a delay.
                            ctx.sleep(POLL_INTERVAL).await?;
                        }
                        Err(InsertCertificateError::Canceled(err)) => {
                            return Err(ctx::Error::Canceled(err))
                        }
                        Err(InsertCertificateError::Inner(err)) => {
                            return Err(ctx::Error::Internal(anyhow::Error::from(err)))
                        }
                    }
                }
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
            .pool
            .connection(ctx)
            .await
            .wrap("connection")?
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
            .pool
            .connection(ctx)
            .await
            .wrap("connection")?
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
        self.pool
            .connection(ctx)
            .await
            .wrap("connection()")?
            .replica_state(ctx)
            .await
            .wrap("replica_state()")
    }

    async fn set_state(&self, ctx: &ctx::Ctx, state: &storage::ReplicaState) -> ctx::Result<()> {
        self.pool
            .connection(ctx)
            .await
            .wrap("connection()")?
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
    /// Get the L1 batch from storage with the highest number.
    async fn last_batch(&self) -> attester::BatchNumber {
        // TODO: Should we return the last batch which was inserted?
        // I think we can assume that there are no gaps in batches, so this should be okay.
        todo!()
    }
    /// Get the L1 batch QC from storage with the highest number.
    async fn last_batch_qc(&self) -> attester::BatchQC {
        // TODO: We discussed that inserting batches should allow gaps, and the retrieval
        // should cater for not returning anything other than what we consider final.
        // Does that apply here, should this one be the last inserted or the last final?
        todo!()
    }
    /// Returns the batch with the given number.
    async fn get_batch(&self, _number: attester::BatchNumber) -> Option<attester::SyncBatch> {
        // TODO: Look up the batch in the store and map it to SyncBatch
        todo!()
    }
    /// Returns the QC of the batch with the given number.
    async fn get_batch_qc(&self, _number: attester::BatchNumber) -> Option<attester::BatchQC> {
        // TODO: Look up the batch QC in the new table. Should this only return the value if it's final?
        todo!()
    }
    /// Store the given QC in the storage.
    async fn store_qc(&self, _qc: attester::BatchQC) {
        // TODO: Insert into the new table even if this creates a gap. Or think about whether this just complicates everything else.
        todo!()
    }
    /// Range of batches persisted in storage.
    fn persisted(&self) -> sync::watch::Receiver<BatchStoreState> {
        // TODO: Add something like the `BlockStoreState` that gets updated when inserts happen.
        // It's not clear whether this gets updated on nodes that aren't actively doing the insertion,
        // but just use the database for synchronising state with other processes.
        todo!()
    }
    /// Queue the batch to be persisted in storage.
    /// `queue_next_batch()` may return BEFORE the batch is actually persisted,
    /// but if the call succeeded the batch is expected to be persisted eventually.
    /// Implementations are only required to accept a batch directly after the previous queued
    /// batch, starting with `persisted().borrow().next()`.
    async fn queue_next_batch(
        &self,
        _ctx: &ctx::Ctx,
        _batch: attester::SyncBatch,
    ) -> ctx::Result<()> {
        // TODO: Check how it works for blocks: does it wait for all previous ones to be persisted here, or in a background queue?
        todo!()
    }
}
