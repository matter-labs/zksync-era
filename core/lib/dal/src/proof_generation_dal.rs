#![doc = include_str!("../doc/ProofGenerationDal.md")]
use std::time::Duration;

use strum::{Display, EnumString};
use zksync_db_connection::{
    connection::Connection, error::DalResult, instrument::Instrumented,
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
                        LEFT JOIN l1_batches ON l1_batch_number = l1_batches.number
                    WHERE
                        (
                            vm_run_data_blob_url IS NOT NULL
                            AND proof_gen_data_blob_url IS NOT NULL
                            AND l1_batches.merkle_root_hash IS NOT NULL
                            AND l1_batches.aux_data_hash IS NOT NULL
                            AND l1_batches.meta_parameters_hash IS NOT NULL
                        )
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

    pub async fn insert_proof_generation_details(
        &mut self,
        block_number: L1BatchNumber,
        proof_gen_data_blob_url: &str,
    ) -> DalResult<()> {
        let l1_batch_number = i64::from(block_number.0);
        let query = sqlx::query!(
            r#"
            INSERT INTO
                proof_generation_details (l1_batch_number, proof_gen_data_blob_url, created_at, updated_at)
            VALUES
                ($1, $2, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            l1_batch_number,
            proof_gen_data_blob_url,
        );
        let instrumentation = Instrumented::new("insert_proof_generation_details")
            .with_arg("l1_batch_number", &l1_batch_number)
            .with_arg("proof_gen_data_blob_url", &proof_gen_data_blob_url);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot save proof_blob_url for a batch number {} that does not exist",
                l1_batch_number
            ));
            return Err(err);
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
                status NOT IN ('picked_by_prover', 'generated')
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
