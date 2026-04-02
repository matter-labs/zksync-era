#![doc = include_str!("../doc/AirbenderProofGenerationDal.md")]
use std::time::Duration;

use chrono::{DateTime, Utc};
use strum::{Display, EnumString};
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
    utils::pg_interval_from_duration,
};
use zksync_types::L1BatchNumber;

use crate::{
    models::storage_airbender_proof::{StorageAirbenderProof, StorageLockedBatch},
    Core,
};

#[derive(Debug)]
pub struct AirbenderProofGenerationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, Clone, Copy, EnumString, Display)]
pub enum AirbenderProofGenerationJobStatus {
    /// The batch has been picked by an Airbender prover and is currently being processed.
    #[strum(serialize = "picked_by_prover")]
    PickedByProver,
    /// The proof has been successfully generated and submitted for the batch.
    #[strum(serialize = "generated")]
    Generated,
    /// The proof generation for the batch has failed, which can happen if its inputs (GCS blob
    /// files) are incomplete or the API is unavailable. Failed batches are retried for a specified
    /// period, as defined in the configuration.
    #[strum(serialize = "failed")]
    Failed,
}

/// Represents a locked batch picked by an Airbender prover. A batch is locked when taken by an Airbender prover
/// ([AirbenderProofGenerationJobStatus::PickedByProver]). It can transition to one of two states:
/// 1. [AirbenderProofGenerationJobStatus::Generated].
/// 2. [AirbenderProofGenerationJobStatus::Failed].
#[derive(Clone, Debug)]
pub struct LockedBatch {
    /// Locked batch number.
    pub l1_batch_number: L1BatchNumber,
    /// The creation time of the job for this batch. It is used to determine if the batch should
    /// transition to [AirbenderProofGenerationJobStatus::Failed].
    pub created_at: DateTime<Utc>,
}

impl AirbenderProofGenerationDal<'_, '_> {
    pub async fn lock_batch_for_proving(
        &mut self,
        processing_timeout: Duration,
        min_batch_number: L1BatchNumber,
    ) -> DalResult<Option<LockedBatch>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let min_batch_number = i64::from(min_batch_number.0);
        let mut transaction = self.storage.start_transaction().await?;

        sqlx::query("LOCK TABLE airbender_proof_generation_details IN EXCLUSIVE MODE")
            .instrument("lock_batch_for_proving#lock")
            .execute(&mut transaction)
            .await?;

        let batch_number = sqlx::query!(
            r#"
            SELECT
                p.l1_batch_number
            FROM
                proof_generation_details p
            LEFT JOIN
                airbender_proof_generation_details apgd
                ON
                    p.l1_batch_number = apgd.l1_batch_number
            WHERE
                (
                    p.l1_batch_number >= $4
                    AND p.vm_run_data_blob_url IS NOT NULL
                    AND p.proof_gen_data_blob_url IS NOT NULL
                )
                AND (
                    apgd.l1_batch_number IS NULL
                    OR (
                        (apgd.status = $1 OR apgd.status = $2)
                        AND apgd.prover_taken_at < NOW() - $3::INTERVAL
                    )
                )
            LIMIT 1
            "#,
            AirbenderProofGenerationJobStatus::PickedByProver.to_string(),
            AirbenderProofGenerationJobStatus::Failed.to_string(),
            processing_timeout,
            min_batch_number
        )
        .instrument("lock_batch_for_proving#get_batch_no")
        .with_arg("processing_timeout", &processing_timeout)
        .with_arg("min_batch_number", &min_batch_number)
        .fetch_optional(&mut transaction)
        .await?;

        let batch_number = match batch_number {
            Some(batch) => batch.l1_batch_number,
            None => {
                return Ok(None);
            }
        };

        let locked_batch = sqlx::query_as!(
            StorageLockedBatch,
            r#"
            INSERT INTO
            airbender_proof_generation_details (
                l1_batch_number, status, created_at, updated_at, prover_taken_at
            )
            VALUES
            (
                $1,
                $2,
                NOW(),
                NOW(),
                NOW()
            )
            ON CONFLICT (l1_batch_number) DO
            UPDATE
            SET
            status = $2,
            updated_at = NOW(),
            prover_taken_at = NOW()
            RETURNING
            l1_batch_number,
            created_at
            "#,
            batch_number,
            AirbenderProofGenerationJobStatus::PickedByProver.to_string(),
        )
        .instrument("lock_batch_for_proving#insert")
        .with_arg("batch_number", &batch_number)
        .fetch_optional(&mut transaction)
        .await?
        .map(Into::into);

        transaction.commit().await?;
        Ok(locked_batch)
    }

    pub async fn unlock_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
        status: AirbenderProofGenerationJobStatus,
    ) -> DalResult<()> {
        let batch_number = i64::from(l1_batch_number.0);
        sqlx::query!(
            r#"
            UPDATE airbender_proof_generation_details
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
            "#,
            status.to_string(),
            batch_number,
        )
        .instrument("unlock_batch")
        .with_arg("l1_batch_number", &batch_number)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn save_proof_artifacts_metadata(
        &mut self,
        batch_number: L1BatchNumber,
        proof_blob_url: &str,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE airbender_proof_generation_details
            SET
                status = $1,
                proof_blob_url = $2,
                updated_at = NOW()
            WHERE
                l1_batch_number = $3
            "#,
            AirbenderProofGenerationJobStatus::Generated.to_string(),
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
                "Updating Airbender proof for a non-existent batch number {} is not allowed",
                batch_number
            ));
            return Err(err);
        }

        Ok(())
    }

    pub async fn get_airbender_proof(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Option<StorageAirbenderProof>> {
        let proof = sqlx::query_as!(
            StorageAirbenderProof,
            r#"
            SELECT
                tp.proof_blob_url,
                tp.updated_at,
                tp.status
            FROM
                airbender_proof_generation_details tp
            WHERE
                tp.l1_batch_number = $1
            "#,
            i64::from(batch_number.0)
        )
        .instrument("get_airbender_proof")
        .with_arg("l1_batch_number", &batch_number)
        .fetch_optional(self.storage)
        .await?;

        Ok(proof)
    }

    /// For testing purposes only.
    pub async fn insert_airbender_proof_generation_job(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            INSERT INTO
            airbender_proof_generation_details (
                l1_batch_number, status, created_at, updated_at
            )
            VALUES
            ($1, $2, NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            batch_number,
            AirbenderProofGenerationJobStatus::PickedByProver.to_string(),
        );
        let instrumentation = Instrumented::new("insert_airbender_proof_generation_job")
            .with_arg("l1_batch_number", &batch_number);
        instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;

        Ok(())
    }

    /// For testing purposes only.
    pub async fn get_oldest_picked_by_prover_batch(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let query = sqlx::query!(
            r#"
            SELECT
                proofs.l1_batch_number
            FROM
                airbender_proof_generation_details AS proofs
            WHERE
                proofs.status = $1
            ORDER BY
                proofs.l1_batch_number ASC
            LIMIT
                1
            "#,
            AirbenderProofGenerationJobStatus::PickedByProver.to_string(),
        );
        let batch_number = Instrumented::new("get_oldest_unpicked_batch")
            .with(query)
            .fetch_optional(self.storage)
            .await?
            .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(batch_number)
    }

    pub async fn get_ready_for_proving_count(
        &mut self,
        min_batch_number: L1BatchNumber,
    ) -> DalResult<i64> {
        let min_batch_number = i64::from(min_batch_number.0);
        let row = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS "count!"
            FROM
                proof_generation_details p
            LEFT JOIN
                airbender_proof_generation_details apgd
                ON p.l1_batch_number = apgd.l1_batch_number
            WHERE
                p.l1_batch_number >= $1
                AND p.vm_run_data_blob_url IS NOT NULL
                AND p.proof_gen_data_blob_url IS NOT NULL
                AND (
                    apgd.l1_batch_number IS NULL
                    OR apgd.status = $2
                )
            "#,
            min_batch_number,
            AirbenderProofGenerationJobStatus::Failed.to_string(),
        )
        .instrument("get_ready_for_proving_count")
        .with_arg("min_batch_number", &min_batch_number)
        .fetch_one(self.storage)
        .await?;

        Ok(row.count)
    }
}
