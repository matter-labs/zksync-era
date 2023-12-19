//! Storage implementation based on DAL.
use std::ops;

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, sync, time};
use zksync_consensus_bft::PayloadSource;
use zksync_consensus_roles::validator;
use zksync_consensus_storage::{BlockStore, ReplicaState, ReplicaStateStore, WriteBlockStore};
use zksync_dal::{consensus_dal::Payload, ConnectionPool};
use zksync_types::{Address};

use crate::consensus;

/// Context-aware `zksync_dal::StorageProcessor` wrapper.
pub(super) struct CtxStorage<'a>(zksync_dal::StorageProcessor<'a>);

impl<'a> CtxStorage<'a> {
    pub async fn access(ctx: &ctx::Ctx, pool: &'a ConnectionPool) -> ctx::Result<Self> {
        Ok(Self(ctx.wait(pool.access_storage_tagged("consensus")).await??))
    }

    pub async fn start_transaction<'b, 'c: 'b>(
        &'c mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<CtxStorage<'b>> {
        Ok(CtxStorage(
            ctx.wait(self.0.start_transaction())
                .await?
                .context("sqlx")?,
        ))
    }

    pub async fn last_miniblock_number(&mut self, ctx: &ctx::Ctx) -> ctx::Result<validator::BlockNumber> {
        let number = ctx
            .wait(self.0.blocks_dal().get_sealed_miniblock_number())
            .await??;
        Ok(validator::BlockNumber(number.0.into()))
    }

    pub async fn commit(self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        Ok(ctx.wait(self.0.commit()).await?.context("sqlx")?)
    }

    pub async fn payload(&mut self, ctx: &ctx::Ctx, number: validator::BlockNumber, operator_address: Address) -> ctx::Result<Option<Payload>> {
        Ok(ctx.wait(self.0.consensus_dal().block_payload(number, operator_address, true)).await??)
    }

    pub async fn certificate(&mut self, ctx: &ctx::Ctx, number: validator::BlockNumber) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx.wait(self.0.consensus_dal().certificate(number)).await??)
    }

    pub async fn first_certificate(&mut self, ctx: &ctx::Ctx) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx.wait(self.0.consensus_dal().first_certificate()).await??)
    }
    
    pub async fn last_certificate(&mut self, ctx: &ctx::Ctx) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx.wait(self.0.consensus_dal().last_certificate()).await??)
    }

    pub async fn replica_state(&mut self, ctx: &ctx::Ctx) -> ctx::Result<ReplicaState> {
        Ok(ctx.wait(self.0.consensus_dal().replica_state()).await??)
    }

    pub async fn set_replica_state(&mut self, ctx: &ctx::Ctx, state: &ReplicaState) -> ctx::Result<()> {
        Ok(ctx.wait(self.0.consensus_dal().set_replica_state(state)).await??)
    }
}

/// Postgres-based [`BlockStore`] implementation, which
/// considers blocks as stored <=> they have consensus field set.
#[derive(Debug)]
pub(super) struct SignedBlockStore {
    first: validator::BlockNumber,
    last_send: sync::Mutex<sync::watch::Sender<validator::BlockNumber>>,
    last_recv: sync::watch::Receiver<validator::BlockNumber>,
    pool: ConnectionPool,
    operator_address: Address,
}

impl SignedBlockStore {
    /// Creates a new storage handle. `pool` should have multiple connections to work efficiently.
    pub async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        validator_key: Option<&validator::SecretKey>,
        operator_address: Address,
    ) -> anyhow::Result<Self> {
        let (first,last) = {
            // Ensure that genesis block has consensus field set in postgres.
            let mut storage = CtxStorage::access(ctx, &pool).await.wrap("access()")?;
            let first = match storage.first_certificate(ctx).await.context("first_certificate()")? {
                Some(first) => first,  
                None => {
                    let Some(validator_key) = validator_key else {
                        anyhow::bail!("genesis not found in storage, and validator key was no provided therefore so we cannot sign genesis");
                    };
                    let number = storage.last_miniblock_number(ctx).await.context("last_miniblock_number()")?; 
                    let payload = storage.payload(ctx,number,operator_address).await.context("payload()")?;
                    let (genesis,_) = zksync_consensus_bft::testonly::make_genesis(&[validator_key.clone()],payload.encode(),number);
                    storage.insert_certificate(ctx,&genesis.justification).await?;
                    genesis.justification
                }
            };
            let last = storage.last_certificate(ctx).await.context("last_certificate()")?
                .context("genesis block disappeared from db")?;
            (first.message.proposal.number,last.message.proposal.number)
        };
        let (last_send,last_recv) = sync::watch::channel(last);
        Ok(Self {
            first,
            last_send,
            last_recv,
            pool,
            operator_address,
        })
    }

    async fn wait_for_previous_blocks(&self, ctx: &ctx::Ctx, block_number: validator::BlockNumber) -> ctx::Result<()> {
        anyhow::ensure!(block_number>=self.first,"block before genesis");
        sync::wait_for(&mut self.last_recv.clone(), |last|last.next()>=block_number).await?;
        Ok(())
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
        self.wait_for_previous_blocks(ctx,block_number).await?;
        let last_send = sync::lock_owned(ctx, &self.last_send).await?;
        if last_send.borrow().next()!=block_number {
            return Err(anyhow::anyhow!("block already finalized").into());
        }
        let storage = &mut CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
        let want = storage.payload(ctx,block_number,self.operator_address).await.wrap("payload()")?.context("miniblock not available")?;
        let got = consensus::Payload::decode(payload).context("consensus::Payload::decode()")?;
        if got != want {
            return Err(anyhow::anyhow!("unexpected payload: got {got:?} want {want:?}").into());
        }
        Ok(())
    }

    /// Puts a block into this storage.
    async fn put_block(&self, ctx: &ctx::Ctx, block: &validator::FinalBlock) -> ctx::Result<()> {
        self.wait_for_previous_blocks(ctx,block.header().number).await?;
        let last_send = sync::lock_owned(ctx, &self.last_send).await?;
        // Early exit if block has been already inserted.
        if *last_send.borrow()>=block.header().number { return Ok(()); }
        // Insert certificate and update `last`.
        let storage = &mut CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
        storage.insert_certificate(ctx,&block.justification).await.wrap("insert_certificate()")?;
        last_send.send_replace(block.header().number);
    }
}

#[async_trait::async_trait]
impl BlockStore for SignedBlockStore {
    async fn head_block(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::FinalBlock> {
        Ok(self.block(ctx,*self.last_recv.borrow()).await?.context("head disappeared from db")?)
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::FinalBlock> {
        Ok(self.block(ctx,self.first).await?.context("genesis disappeared from db")?)
    }

    async fn last_contiguous_block_number(&self,ctx: &ctx::Ctx) -> ctx::Result<validator::BlockNumber> {
        Ok(*self.last_recv.borrow())
    }

    async fn block(&self, ctx: &ctx::Ctx, number: validator::BlockNumber) -> ctx::Result<Option<validator::FinalBlock>> {
        let storage = &mut CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
        let Some(justification) = storage.certificate(ctx,number).await.wrap("certificate()")? else { return Ok(None) };
        let payload = storage.payload(ctx,number,self.operator_address).await.wrap("payload()")?.context("miniblock disappeared from storage")?;
        Ok(Some(validator::FinalBlock{payload,justification}))
    }

    async fn missing_block_numbers(&self, ctx: &ctx::Ctx, range: ops::Range<validator::BlockNumber>) -> ctx::Result<Vec<validator::BlockNumber>> {
        let last = *self.last_recv.borrow();
        let mut output = vec![];
        output.extend((range.start.0..self.genesis.0).map(validator::BlockNumber));
        output.extend((last.next().0..range.end.0).map(validator::BlockNumber));
        Ok(output)
    }

    fn subscribe_to_block_writes(&self) -> sync::watch::Receiver<validator::BlockNumber> {
        self.last_recv.clone()
    }
}

#[async_trait::async_trait]
impl ReplicaStateStore for SignedBlockStore {
    async fn replica_state(&self, ctx: &ctx::Ctx) -> ctx::Result<Option<ReplicaState>> {
        let storage = &mut CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
        storage.replica_state(ctx).await
    }

    async fn put_replica_state(&self, ctx: &ctx::Ctx, replica_state: &ReplicaState) -> ctx::Result<()> {
        let storage = &mut CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
        storage.set_replica_state(ctx,replica_state).await
    }
}

#[async_trait::async_trait]
impl PayloadSource for SignedBlockStore {
    async fn propose(&self,ctx: &ctx::Ctx, block_number: validator::BlockNumber) -> ctx::Result<validator::Payload> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        self.wait_for_previous_blocks(ctx,block_number).await?;
        loop {
            let storage = &mut CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
            if let Some(payload) = storage.payload(ctx, block_number, self.operator_address).await.wrap("payload()")? {
                return Ok(payload.encode());
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }
}
