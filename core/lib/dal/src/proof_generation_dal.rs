#![doc = include_str!("../doc/ProofGenerationDal.md")]
use std::time::Duration;

use strum::{Display, EnumString};
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
    utils::pg_interval_from_duration,
};
use zksync_types::L1BatchNumber;

use crate::Core;

#[derive(Debug)]
pub struct ProofGenerationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, EnumString, Display)]
enum ProofGenerationJobStatus {
    #[strum(serialize = "ready_to_be_proven")]
    ReadyToBeProven,
    #[strum(serialize = "picked_by_prover")]
    PickedByProver,
    #[strum(serialize = "generated")]
    Generated,
    #[strum(serialize = "skipped")]
    Skipped,
}

impl ProofGenerationDal<'_, '_> {
    pub async fn get_next_block_to_be_proven(
        &mut self,
        processing_timeout: Duration,
    ) -> DalResult<Option<L1BatchNumber>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                status = 'picked_by_prover',
                updated_at = NOW(),
                prover_taken_at = NOW()
            WHERE
                l1_batch_number = (
                    SELECT
                        l1_batch_number
                    FROM
                        proof_generation_details
                    WHERE
                        status = 'ready_to_be_proven'
                        OR (
                            status = 'picked_by_prover'
                            AND prover_taken_at < NOW() - $1::INTERVAL
                        )
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                proof_generation_details.l1_batch_number
            "#,
            &processing_timeout,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(result)
    }

    pub async fn save_proof_artifacts_metadata(
        &mut self,
        batch_number: L1BatchNumber,
        proof_blob_url: &str,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                status = 'generated',
                proof_blob_url = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            proof_blob_url,
            batch_number
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("proof_blob_url", &proof_blob_url)
            .with_arg("l1_batch_number", &batch_number);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot save proof_blob_url for a batch number {} that does not exist",
                batch_number
            ));
            return Err(err);
        }

        Ok(())
    }

    /// The caller should ensure that `l1_batch_number` exists in the database.
    pub async fn insert_proof_generation_details(
        &mut self,
        l1_batch_number: L1BatchNumber,
        proof_gen_data_blob_url: &str,
    ) -> DalResult<()> {
        let result = sqlx::query!(
            r#"
            INSERT INTO
                proof_generation_details (l1_batch_number, status, proof_gen_data_blob_url, created_at, updated_at)
            VALUES
                ($1, 'ready_to_be_proven', $2, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
             i64::from(l1_batch_number.0),
            proof_gen_data_blob_url,
        )
        .instrument("insert_proof_generation_details")
        .with_arg("l1_batch_number", &l1_batch_number)
        .with_arg("proof_gen_data_blob_url", &proof_gen_data_blob_url)
        .report_latency()
        .execute(self.storage)
        .await?;

        if result.rows_affected() == 0 {
            // Not an error: we may call `insert_proof_generation_details()` from multiple full trees instantiated
            // for the same node. Unlike tree data, we don't particularly care about correspondence of `proof_gen_data_blob_url` across calls,
            // so just log this fact and carry on.
            tracing::debug!("L1 batch #{l1_batch_number}: proof generation data wasn't updated as it's already present");
        }
        Ok(())
    }

    pub async fn mark_proof_generation_job_as_skipped(
        &mut self,
        block_number: L1BatchNumber,
    ) -> DalResult<()> {
        let status = ProofGenerationJobStatus::Skipped.to_string();
        let l1_batch_number = i64::from(block_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            status,
            l1_batch_number
        );
        let instrumentation = Instrumented::new("mark_proof_generation_job_as_skipped")
            .with_arg("status", &status)
            .with_arg("l1_batch_number", &l1_batch_number);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot mark proof as skipped because batch number {} does not exist",
                l1_batch_number
            ));
            return Err(err);
        }

        Ok(())
    }

    pub async fn get_oldest_unpicked_batch(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                proof_generation_details
            WHERE
                status = 'ready_to_be_proven'
            ORDER BY
                l1_batch_number ASC
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(result)
    }

    pub async fn get_oldest_not_generated_batch(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                proof_generation_details
            WHERE
                status NOT IN ('generated', 'skipped')
            ORDER BY
                l1_batch_number ASC
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::ProtocolVersion;

    use super::*;
    use crate::{tests::create_l1_batch_header, ConnectionPool, CoreDal};

    #[tokio::test]
    async fn proof_generation_workflow() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_mock_l1_batch(&create_l1_batch_header(1))
            .await
            .unwrap();

        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, None);

        conn.proof_generation_dal()
            .insert_proof_generation_details(L1BatchNumber(1), "generation_data")
            .await
            .unwrap();

        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, Some(L1BatchNumber(1)));

        // Calling the method multiple times should work fine.
        conn.proof_generation_dal()
            .insert_proof_generation_details(L1BatchNumber(1), "generation_data")
            .await
            .unwrap();

        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, Some(L1BatchNumber(1)));

        let picked_l1_batch = conn
            .proof_generation_dal()
            .get_next_block_to_be_proven(Duration::MAX)
            .await
            .unwrap();
        assert_eq!(picked_l1_batch, Some(L1BatchNumber(1)));
        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, None);

        // Check that with small enough processing timeout, the L1 batch can be picked again
        let picked_l1_batch = conn
            .proof_generation_dal()
            .get_next_block_to_be_proven(Duration::ZERO)
            .await
            .unwrap();
        assert_eq!(picked_l1_batch, Some(L1BatchNumber(1)));

        conn.proof_generation_dal()
            .save_proof_artifacts_metadata(L1BatchNumber(1), "proof")
            .await
            .unwrap();

        let picked_l1_batch = conn
            .proof_generation_dal()
            .get_next_block_to_be_proven(Duration::MAX)
            .await
            .unwrap();
        assert_eq!(picked_l1_batch, None);
        let unpicked_l1_batch = conn
            .proof_generation_dal()
            .get_oldest_unpicked_batch()
            .await
            .unwrap();
        assert_eq!(unpicked_l1_batch, None);
    }
}
