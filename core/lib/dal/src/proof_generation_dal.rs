#![doc = include_str!("../doc/ProofGenerationDal.md")]
use std::time::Duration;

use strum::{Display, EnumString};
use zksync_db_connection::{connection::Connection, utils::pg_interval_from_duration};
use zksync_types::L1BatchNumber;

use crate::{Core, SqlxError};

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
    ) -> Option<L1BatchNumber> {
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

        result
    }

    pub async fn save_proof_artifacts_metadata(
        &mut self,
        block_number: L1BatchNumber,
        proof_blob_url: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query!(
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
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await?
        .rows_affected()
        .eq(&1)
        .then_some(())
        .ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn insert_proof_generation_details(
        &mut self,
        block_number: L1BatchNumber,
        proof_gen_data_blob_url: &str,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
                proof_generation_details (l1_batch_number, status, proof_gen_data_blob_url, created_at, updated_at)
            VALUES
                ($1, 'ready_to_be_proven', $2, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            i64::from(block_number.0),
            proof_gen_data_blob_url,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn mark_proof_generation_job_as_skipped(
        &mut self,
        block_number: L1BatchNumber,
    ) -> Result<(), SqlxError> {
        sqlx::query!(
            r#"
            UPDATE proof_generation_details
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            ProofGenerationJobStatus::Skipped.to_string(),
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await?
        .rows_affected()
        .eq(&1)
        .then_some(())
        .ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn get_oldest_unpicked_batch(&mut self) -> Option<L1BatchNumber> {
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

        result
    }

    pub async fn get_oldest_not_generated_batch(&mut self) -> Option<L1BatchNumber> {
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

        result
    }
}
