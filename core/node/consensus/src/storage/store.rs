use std::sync::Arc;

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, sync};
use zksync_consensus_bft::PayloadManager;
use zksync_consensus_roles::validator;
use zksync_consensus_storage as storage;
use zksync_dal::consensus_dal::Payload;
use zksync_node_sync::fetcher::{FetchedBlock, FetchedTransaction};
use zksync_types::L2BlockNumber;

use super::PayloadQueue;
use crate::storage::ConnectionPool;

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
    payloads: Arc<sync::Mutex<Option<PayloadQueue>>>,
    /// L2 block QCs received over gossip
    certificates: ctx::channel::UnboundedSender<validator::CommitQC>,
    /// Range of L2 blocks for which we have a QC persisted.
    persisted: sync::watch::Receiver<storage::BlockStoreState>,
}

/// Background task of the `Store`.
pub struct StoreRunner {
    pool: ConnectionPool,
    persisted: sync::watch::Sender<storage::BlockStoreState>,
    certificates: ctx::channel::UnboundedReceiver<validator::CommitQC>,
}

impl Store {
    pub async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        payload_queue: Option<PayloadQueue>,
    ) -> ctx::Result<(Store, StoreRunner)> {
        let persisted = pool
            .connection(ctx)
            .await
            .wrap("connection()")?
            .certificates_range(ctx)
            .await
            .wrap("certificates_range()")?;
        let persisted = sync::watch::channel(persisted).0;
        let (certs_send, certs_recv) = ctx::channel::unbounded();
        Ok((
            Store {
                pool: pool.clone(),
                certificates: certs_send,
                payloads: Arc::new(sync::Mutex::new(payload_queue)),
                persisted: persisted.subscribe(),
            },
            StoreRunner {
                pool,
                persisted,
                certificates: certs_recv,
            },
        ))
    }
}

impl StoreRunner {
    /// Background tasks for the store:
    /// * save L2 quorum certificates into the persistent backend
    pub async fn run(mut self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let res = async {
            loop {
                let cert = self.certificates.recv(ctx).await?;
                self.pool
                    .wait_for_payload(ctx, cert.header().number)
                    .await
                    .wrap("wait_for_payload()")?;
                self.pool
                    .connection(ctx)
                    .await
                    .wrap("connection()")?
                    .insert_certificate(ctx, &cert)
                    .await
                    .wrap("insert_certificate()")?;
                self.persisted.send_modify(|p| p.last = Some(cert));
            }
        }
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
        self.persisted.clone()
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
        let mut payloads = sync::lock(ctx, &self.payloads).await?.into_async();
        if let Some(payloads) = &mut *payloads {
            payloads
                .send(to_fetched_block(block.number(), &block.payload).context("to_fetched_block")?)
                .await
                .context("payload_queue.send()")?;
        }
        self.certificates.send(block.justification);
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
        let payload = self.pool.wait_for_payload(ctx, block_number).await?;
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
        let mut payloads = sync::lock(ctx, &self.payloads).await?.into_async();
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
