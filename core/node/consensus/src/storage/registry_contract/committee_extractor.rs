use anyhow::Context;
use zksync_concurrency::{ctx, ctx::Ctx, scope};
use zksync_consensus_roles::{attester, validator};
use zksync_node_api_server::execution_sandbox::BlockArgs;
use zksync_types::api;

use crate::storage::{
    registry_contract::{vm_reader::VMReader},
    ConnectionPool,
};

#[derive(Debug)]
pub struct CommitteeExtractor {
    pool: ConnectionPool,
    reader: VMReader,
}

impl CommitteeExtractor {
    pub fn new(pool: ConnectionPool, reader: VMReader) -> CommitteeExtractor {
        Self { pool, reader }
    }

    pub async fn run(mut self, ctx: &Ctx) -> anyhow::Result<()> {
        tracing::info!("Running zksync_node_consensus::storage::CommitteeExtractor");
        let res = scope::run!(ctx, |ctx, s| async {
            let mut next_batch = self.next_batch_to_extract_committee(ctx).await?;
            loop {
                self.wait_batch(ctx, next_batch).await?;
                self.extract_batch_committee(ctx, next_batch).await?;
                next_batch = next_batch.next();
            }
        })
        .await;
        match res {
            Ok(()) | Err(ctx::Error::Canceled(_)) => Ok(()),
            Err(ctx::Error::Internal(err)) => Err(err),
        }
    }

    async fn batch_max_l2_block_args(&self, ctx: &Ctx, n: attester::BatchNumber) -> anyhow::Result<BlockArgs> {
        let max_l2_block = self.get_max_l2_block_of_l1_batch(ctx, n).await?.unwrap();
        let block_id = api::BlockId::Number(api::BlockNumber::Number(max_l2_block.0.into()));
        self.reader.block_args(ctx, block_id).await.context("block_args()")
    }

    async fn extract_attester_committee(
        &mut self,
        ctx: &Ctx,
        batch_number: attester::BatchNumber,
    ) -> ctx::Result<()> {
        let block_args = self.batch_max_l2_block_args(ctx, batch_number).await?;
        let committee = self
            .reader
            .read_attester_committee(block_args)
            .await
            .context("read_attester_committee()")?;
        self.pool.connection(ctx).await.context("connection")?.insert_attester_committee(ctx, batch_number + 1, committee).await?;
        Ok(())
    }

    async fn next_batch_to_extract_committee(
        &mut self,
        ctx: &Ctx,
    ) -> ctx::Result<attester::BatchNumber> {
        let mut conn = self.pool.connection(ctx).await?;
        conn.next_batch_to_extract_committee(ctx).await
    }

    async fn get_max_l2_block_of_l1_batch(
        &mut self,
        ctx: &Ctx,
        batch_number: attester::BatchNumber,
    ) -> ctx::Result<Option<validator::BlockNumber>> {
        let mut conn = self.pool.connection(ctx).await?;
        let Some((_, max)) = conn
            .get_l2_block_range_of_l1_batch(ctx, batch_number)
            .await
            .context("get_l2_block_range_of_l1_batch()")?
        else {
            return Ok(None);
        };

        Ok(Some(max))
    }
}
