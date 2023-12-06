//! Storage implementation based on DAL.
use crate::consensus;
use anyhow::Context as _;
use std::ops;
use zksync_concurrency::{ctx, error::Wrap as _, sync, time};
use zksync_consensus_bft::PayloadSource;
use zksync_consensus_roles::validator;
use zksync_consensus_storage::{BlockStore, ReplicaState, ReplicaStateStore, WriteBlockStore};
use zksync_dal::{blocks_dal::ConsensusBlockFields, ConnectionPool};
use zksync_types::{api::en::SyncBlock, Address, MiniblockNumber};

pub(crate) fn sync_block_to_consensus_block(
    block: SyncBlock,
) -> anyhow::Result<validator::FinalBlock> {
    let number = validator::BlockNumber(block.number.0.into());
    let consensus = ConsensusBlockFields::decode(
        block
            .consensus
            .as_ref()
            .context("Missing consensus fields")?,
    )
    .context("ConsensusBlockFields::decode()")?;
    let consensus_protocol_version = consensus.justification.message.protocol_version.as_u32();
    let block_protocol_version = block.protocol_version as u32;
    anyhow::ensure!(
        consensus_protocol_version == block_protocol_version,
        "Protocol version for justification ({consensus_protocol_version}) differs from \
         SyncBlock.protocol_version={block_protocol_version}"
    );

    let payload: consensus::Payload = block.try_into()?;
    let payload = payload.encode();
    Ok(validator::FinalBlock {
        header: validator::BlockHeader {
            parent: consensus.parent,
            number,
            payload: payload.hash(),
        },
        payload,
        justification: consensus.justification,
    })
}

/// Context-aware `zksync_dal::StorageProcessor` wrapper.
pub(super) struct StorageProcessor<'a>(zksync_dal::StorageProcessor<'a>);

pub(super) async fn storage<'a>(
    ctx: &ctx::Ctx,
    pool: &'a ConnectionPool,
) -> ctx::Result<StorageProcessor<'a>> {
    Ok(StorageProcessor(
        ctx.wait(pool.access_storage_tagged("sync_layer")).await??,
    ))
}

impl<'a> StorageProcessor<'a> {
    pub async fn start_transaction<'b, 'c: 'b>(
        &'c mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<StorageProcessor<'b>> {
        Ok(StorageProcessor(
            ctx.wait(self.0.start_transaction())
                .await?
                .context("sqlx")?,
        ))
    }

    pub async fn commit(self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        Ok(ctx.wait(self.0.commit()).await?.context("sqlx")?)
    }

    pub async fn fetch_block(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
        operator_address: Address,
    ) -> ctx::Result<Option<validator::FinalBlock>> {
        let number = MiniblockNumber(number.0.try_into().context("MiniblockNumber")?);
        let Some(block) = ctx
            .wait(self.0.sync_dal().sync_block(number, operator_address, true))
            .await?
            .context("sync_block()")?
        else {
            return Ok(None);
        };
        if block.consensus.is_none() {
            return Ok(None);
        }
        Ok(Some(
            sync_block_to_consensus_block(block).context("sync_block_to_consensus_block()")?,
        ))
    }

    pub async fn fetch_payload(
        &mut self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
        operator_address: Address,
    ) -> ctx::Result<Option<consensus::Payload>> {
        let n = MiniblockNumber(block_number.0.try_into().context("MiniblockNumber")?);
        let Some(sync_block) = ctx
            .wait(self.0.sync_dal().sync_block(n, operator_address, true))
            .await?
            .context("sync_block()")?
        else {
            return Ok(None);
        };
        Ok(Some(sync_block.try_into()?))
    }

    pub async fn put_block(
        &mut self,
        ctx: &ctx::Ctx,
        block: &validator::FinalBlock,
        operator_address: Address,
    ) -> ctx::Result<()> {
        tracing::error!("put_block({:?})", block.header.number);
        let n = MiniblockNumber(
            block
                .header
                .number
                .0
                .try_into()
                .context("MiniblockNumber")?,
        );
        let mut txn = self
            .start_transaction(ctx)
            .await
            .wrap("start_transaction()")?;

        // We require the block to be already stored in Postgres when we set the consensus field.
        let sync_block = ctx
            .wait(txn.0.sync_dal().sync_block(n, operator_address, true))
            .await?
            .context("sync_block()")?
            .context("unknown block")?;
        let want = &ConsensusBlockFields {
            parent: block.header.parent,
            justification: block.justification.clone(),
        };

        // Early exit if consensus field is already set to the expected value.
        if Some(want.encode()) == sync_block.consensus {
            return Ok(());
        }

        // Verify that the payload matches the storage.
        let want_payload: consensus::Payload = sync_block.try_into()?;
        if want_payload.encode() != block.payload {
            let got_payload = consensus::Payload::decode(&block.payload)?;
            return Err(anyhow::anyhow!(
                "payload mismatch: got {got_payload:?}, want {want_payload:?}"
            )
            .into());
        }

        ctx.wait(txn.0.blocks_dal().set_miniblock_consensus_fields(n, want))
            .await?
            .context("set_miniblock_consensus_fields()")?;
        txn.commit(ctx).await.wrap("commit()")?;
        Ok(())
    }

    pub async fn find_head_number(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<validator::BlockNumber> {
        let head = ctx
            .wait(
                self.0
                    .blocks_dal()
                    .get_last_miniblock_number_with_consensus_fields(),
            )
            .await?
            .context("get_last_miniblock_number_with_consensus_fields()")?
            .context("head not found")?;
        Ok(validator::BlockNumber(head.0.into()))
    }

    pub async fn find_head_forward(
        &mut self,
        ctx: &ctx::Ctx,
        start_at: validator::BlockNumber,
        operator_address: Address,
    ) -> ctx::Result<validator::FinalBlock> {
        let mut block = self
            .fetch_block(ctx, start_at, operator_address)
            .await
            .wrap("fetch_block()")?
            .context("head not found")?;
        while let Some(next) = self
            .fetch_block(ctx, block.header.number.next(), operator_address)
            .await
            .wrap("fetch_block()")?
        {
            block = next;
        }
        Ok(block)
    }
}

/// Postgres-based [`BlockStore`] implementation, which
/// considers blocks as stored <=> they have consensus field set.
#[derive(Debug)]
pub(super) struct SignedBlockStore {
    genesis: validator::BlockNumber,
    head: sync::watch::Sender<validator::BlockNumber>,
    pool: ConnectionPool,
    operator_address: Address,
}

impl SignedBlockStore {
    /// Creates a new storage handle. `pool` should have multiple connections to work efficiently.
    pub async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        genesis: &validator::FinalBlock,
        operator_address: Address,
    ) -> anyhow::Result<Self> {
        // Ensure that genesis block has consensus field set in postgres.
        let head = {
            let mut storage = storage(ctx, &pool).await.wrap("storage()")?;
            storage
                .put_block(ctx, genesis, operator_address)
                .await
                .wrap("put_block()")?;

            // Find the last miniblock with consensus field set (aka head).
            // We assume here that all blocks in range (genesis,head) also have consensus field set.
            // WARNING: genesis should NEVER be moved to an earlier block.
            storage
                .find_head_number(ctx)
                .await
                .wrap("find_head_number()")?
        };
        Ok(Self {
            genesis: genesis.header.number,
            head: sync::watch::channel(head).0,
            pool,
            operator_address,
        })
    }
}

#[async_trait::async_trait]
impl WriteBlockStore for SignedBlockStore {
    /// Verify that `payload` is a correct proposal for the block `block_number`.
    async fn verify_payload(
        &self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
        payload: &validator::Payload,
    ) -> ctx::Result<()> {
        tracing::error!("SignedBlockStore::verify_payload({block_number:?})");
        let storage = &mut storage(ctx, &self.pool).await.wrap("storage()")?;
        let want = storage
            .fetch_payload(ctx, block_number, self.operator_address)
            .await
            .wrap("fetch_payload()")?
            .context("unknown block")?;
        if payload != &want.encode() {
            return Err(anyhow::anyhow!("unexpected payload").into());
        }
        Ok(())
    }

    /// Puts a block into this storage.
    async fn put_block(&self, ctx: &ctx::Ctx, block: &validator::FinalBlock) -> ctx::Result<()> {
        tracing::error!("SignedBlockStore::put_block()");
        let storage = &mut storage(ctx, &self.pool).await.wrap("storage()")?;

        // Currently main node is the only validator, so it should be the only one creating new
        // blocks. To ensure that no gaps in the blocks are created we check here that we always
        // insert a new head.
        let head = *self.head.borrow();
        let head = storage
            .find_head_forward(ctx, head, self.operator_address)
            .await
            .wrap("find_head_forward()")?;
        if block.header.number != head.header.number.next() {
            return Err(anyhow::anyhow!(
                "expected block with number {}, got {}",
                head.header.number.next(),
                block.header.number
            )
            .into());
        }

        storage
            .put_block(ctx, block, self.operator_address)
            .await
            .wrap("put_block()")
    }
}

#[async_trait::async_trait]
impl BlockStore for SignedBlockStore {
    async fn head_block(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::FinalBlock> {
        let storage = &mut storage(ctx, &self.pool).await.wrap("storage()")?;
        let head = *self.head.borrow();
        storage
            .find_head_forward(ctx, head, self.operator_address)
            .await
            .wrap("find_head_forward()")
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::FinalBlock> {
        let storage = &mut storage(ctx, &self.pool).await.wrap("storage()")?;
        Ok(storage
            .fetch_block(ctx, self.genesis, self.operator_address)
            .await
            .wrap("fetch_block()")?
            .context("Genesis miniblock not present in Postgres")?)
    }

    async fn last_contiguous_block_number(
        &self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<validator::BlockNumber> {
        Ok(self.head_block(ctx).await?.header.number)
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::FinalBlock>> {
        let storage = &mut storage(ctx, &self.pool).await.wrap("storage()")?;
        storage
            .fetch_block(ctx, number, self.operator_address)
            .await
            .wrap("fetch_block()")
    }

    async fn missing_block_numbers(
        &self,
        _ctx: &ctx::Ctx,
        _range: ops::Range<validator::BlockNumber>,
    ) -> ctx::Result<Vec<validator::BlockNumber>> {
        Ok(vec![]) // The storage never has missing blocks by construction
    }

    fn subscribe_to_block_writes(&self) -> sync::watch::Receiver<validator::BlockNumber> {
        self.head.subscribe()
    }
}

#[async_trait::async_trait]
impl ReplicaStateStore for SignedBlockStore {
    async fn replica_state(&self, ctx: &ctx::Ctx) -> ctx::Result<Option<ReplicaState>> {
        let storage = &mut storage(ctx, &self.pool).await.wrap("storage")?;
        Ok(ctx
            .wait(storage.0.consensus_dal().replica_state())
            .await?
            .context("replica_state()")?)
    }

    async fn put_replica_state(
        &self,
        ctx: &ctx::Ctx,
        replica_state: &ReplicaState,
    ) -> ctx::Result<()> {
        let storage = &mut storage(ctx, &self.pool).await.wrap("storage")?;
        Ok(ctx
            .wait(storage.0.consensus_dal().put_replica_state(replica_state))
            .await?
            .context("put_replica_state()")?)
    }
}

#[async_trait::async_trait]
impl PayloadSource for SignedBlockStore {
    async fn propose(
        &self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
    ) -> ctx::Result<validator::Payload> {
        tracing::error!("propose({block_number:?})");
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            let storage = &mut storage(ctx, &self.pool).await.wrap("storage()")?;
            if let Some(payload) = storage
                .fetch_payload(ctx, block_number, self.operator_address)
                .await
                .wrap("fetch_payload()")?
            {
                return Ok(payload.encode());
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }
}

impl SignedBlockStore {
    pub async fn run_background_tasks(&self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        let mut head = *self.head.borrow();
        let res: ctx::Result<()> = async {
            loop {
                let storage = &mut storage(ctx, &self.pool).await.wrap("storage()")?;
                head = storage
                    .find_head_forward(ctx, head, self.operator_address)
                    .await
                    .wrap("find_head_forward()")?
                    .header
                    .number;
                self.head.send_if_modified(|x| {
                    if *x >= head {
                        return false;
                    }
                    *x = head;
                    true
                });
                ctx.sleep(POLL_INTERVAL).await?;
            }
        }
        .await;
        match res.err().unwrap() {
            ctx::Error::Canceled(_) => Ok(()),
            ctx::Error::Internal(err) => Err(err),
        }
    }
}
