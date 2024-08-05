use anyhow::Context;
use zksync_concurrency::{ctx, ctx::Ctx, scope, time};
use zksync_consensus_roles::{attester, attester::BatchNumber, validator};
use zksync_dal::consensus_dal;
use zksync_node_api_server::execution_sandbox::BlockArgs;

use crate::storage::{vm_reader::VMReader, CommitteeAttester, CommitteeValidator, ConnectionPool};

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

    async fn batch_max_l2_block_args(
        &mut self,
        ctx: &Ctx,
        batch_number: attester::BatchNumber,
    ) -> anyhow::Result<BlockArgs> {
        let max_l2_block = self
            .get_max_l2_block_of_l1_batch(ctx, batch_number)
            .await?
            .unwrap();

        let block_number =
            zksync_types::api::BlockNumber::Number(zksync_types::U64::from(max_l2_block.0));
        let block_id = zksync_types::api::BlockId::Number(block_number);

        self.reader
            .block_args(ctx, block_id)
            .await
            .context("block_args()")
    }

    async fn extract_batch_committee(
        &mut self,
        ctx: &Ctx,
        batch_number: attester::BatchNumber,
    ) -> ctx::Result<()> {
        let block_args = self.batch_max_l2_block_args(ctx, batch_number).await?;
        let contract_attester_committee = match self.reader.contract_deployed(block_args).await {
            false => vec![],
            true => self
                .reader
                .read_attester_committee(block_args)
                .await
                .context("read_attester_committee()")?,
        };

        let db_attester_committee = consensus_dal::AttesterCommittee {
            members: contract_attester_committee
                .into_iter()
                .map(CommitteeAttester::try_into)
                .collect::<Result<Vec<consensus_dal::Attester>, _>>()?,
        };

        self.pool
            .connection(ctx)
            .await?
            .insert_batch_committee(ctx, batch_number, db_attester_committee)
            .await?;

        Ok(())
    }

    async fn next_batch_to_extract_committee(
        &mut self,
        ctx: &Ctx,
    ) -> ctx::Result<attester::BatchNumber> {
        let mut conn = self.pool.connection(ctx).await?;
        conn.next_batch_to_extract_committee(ctx).await
    }

    async fn wait_batch(
        &mut self,
        ctx: &Ctx,
        batch_number: attester::BatchNumber,
    ) -> ctx::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(200);
        loop {
            let mut conn = self.pool.connection(ctx).await?;
            if let Some(last_batch_number) = conn
                .get_last_batch_number(ctx)
                .await
                .context("get_last_batch_number()")?
            {
                if last_batch_number >= batch_number {
                    return Ok(());
                }
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
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
