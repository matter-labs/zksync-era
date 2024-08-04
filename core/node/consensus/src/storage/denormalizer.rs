use anyhow::Context;
use zksync_concurrency::{ctx, ctx::Ctx, scope, time};
use zksync_consensus_roles::{attester, validator};
use zksync_dal::consensus_dal;

use crate::storage::{vm_reader::VMReader, CommitteeAttester, CommitteeValidator, ConnectionPool};

#[derive(Debug)]
pub struct Denormalizer {
    pool: ConnectionPool,
    reader: VMReader,
}

impl Denormalizer {
    pub fn new(pool: ConnectionPool, reader: VMReader) -> Denormalizer {
        Self { pool, reader }
    }

    pub async fn run(mut self, ctx: &Ctx) -> anyhow::Result<()> {
        tracing::info!("Running zksync_node_consensus::storage::Denormalizer");
        let res = scope::run!(ctx, |ctx, s| async {
            let mut next_batch = self.last_batch_committees(ctx).await?.next();
            loop {
                self.wait_batch(ctx, next_batch).await?;
                self.denormalize_batch_committees(ctx, next_batch).await?;
                next_batch = next_batch.next();
            }
        })
        .await;
        match res {
            Ok(()) | Err(ctx::Error::Canceled(_)) => Ok(()),
            Err(ctx::Error::Internal(err)) => Err(err),
        }
    }

    async fn denormalize_batch_committees(
        &mut self,
        ctx: &Ctx,
        batch_number: attester::BatchNumber,
    ) -> ctx::Result<()> {
        let max_l2_block = self
            .get_max_l2_block_of_l1_batch(ctx, batch_number)
            .await?
            .unwrap();

        let block_number =
            zksync_types::api::BlockNumber::Number(zksync_types::U64::from(max_l2_block.0));
        let block_id = zksync_types::api::BlockId::Number(block_number);
        let (validator_committee, attester_committee) =
            self.reader.read_committees(ctx, block_id).await?;

        let (validator_committee, attester_committee) =
            Self::transform_committees((validator_committee, attester_committee))?;

        self.pool
            .connection(ctx)
            .await?
            .insert_batch_committees(ctx, batch_number, validator_committee, attester_committee)
            .await?;

        Ok(())
    }

    async fn last_batch_committees(&mut self, ctx: &Ctx) -> ctx::Result<attester::BatchNumber> {
        // TODO(moshababo): implement
        Ok(attester::BatchNumber(0))
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

    fn transform_committees(
        committees: (Vec<CommitteeValidator>, Vec<CommitteeAttester>),
    ) -> ctx::Result<(
        consensus_dal::ValidatorCommittee,
        consensus_dal::AttesterCommittee,
    )> {
        let validator_committee = consensus_dal::ValidatorCommittee {
            members: committees
                .0
                .into_iter()
                .map(CommitteeValidator::try_into)
                .collect::<Result<Vec<consensus_dal::Validator>, _>>()?,
        };

        let attester_committee = consensus_dal::AttesterCommittee {
            members: committees
                .1
                .into_iter()
                .map(CommitteeAttester::try_into)
                .collect::<Result<Vec<consensus_dal::Attester>, _>>()?,
        };

        Ok((validator_committee, attester_committee))
    }
}
