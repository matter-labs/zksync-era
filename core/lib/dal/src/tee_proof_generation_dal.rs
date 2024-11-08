#![doc = include_str!("../doc/TeeProofGenerationDal.md")]
use std::time::Duration;

use chrono::{DateTime, Utc};
use strum::{Display, EnumString};
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
    utils::pg_interval_from_duration,
};
use zksync_types::{tee_types::TeeType, L1BatchNumber};

use crate::{
    models::storage_tee_proof::{StorageLockedBatch, StorageTeeProof},
    Core,
};

#[derive(Debug)]
pub struct TeeProofGenerationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, Clone, Copy, EnumString, Display)]
pub enum TeeProofGenerationJobStatus {
    #[strum(serialize = "picked_by_prover")]
    PickedByProver,
    #[strum(serialize = "generated")]
    Generated,
    #[strum(serialize = "failed")]
    Failed,
    #[strum(serialize = "permanently_ignored")]
    PermanentlyIgnored,
}

/// Represents a locked batch picked by a TEE prover. A batch is locked when taken by a TEE prover
/// ([TeeProofGenerationJobStatus::PickedByProver]). It can transition to one of three states:
/// 1. [TeeProofGenerationJobStatus::Generated] when the proof is successfully submitted.
/// 2. [TeeProofGenerationJobStatus::Failed] when the proof generation fails, which can happen if
///    its inputs (GCS blob files) are incomplete or the API is unavailable for an extended period.
/// 3. [TeeProofGenerationJobStatus::PermanentlyIgnored] when the proof generation has been
///    continuously failing for an extended period.
#[derive(Clone, Debug)]
pub struct LockedBatch {
    /// Locked batch number.
    pub l1_batch_number: L1BatchNumber,
    /// The creation time of the job for this batch. It is used to determine if the batch should
    /// transition to [TeeProofGenerationJobStatus::PermanentlyIgnored] or [TeeProofGenerationJobStatus::Failed].
    pub created_at: DateTime<Utc>,
}

impl TeeProofGenerationDal<'_, '_> {
    pub async fn lock_batch_for_proving(
        &mut self,
        tee_type: TeeType,
        processing_timeout: Duration,
        min_batch_number: L1BatchNumber,
    ) -> DalResult<Option<LockedBatch>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let min_batch_number = i64::from(min_batch_number.0);
        let locked_batch = sqlx::query_as!(
            StorageLockedBatch,
            r#"
            WITH upsert AS (
                SELECT
                    p.l1_batch_number
                FROM
                    proof_generation_details p
                LEFT JOIN
                    tee_proof_generation_details tee
                    ON
                        p.l1_batch_number = tee.l1_batch_number
                        AND tee.tee_type = $1
                WHERE
                    (
                        p.l1_batch_number >= $5
                        AND p.vm_run_data_blob_url IS NOT NULL
                        AND p.proof_gen_data_blob_url IS NOT NULL
                    )
                    AND (
                        tee.l1_batch_number IS NULL
                        OR (
                            (tee.status = $2 OR tee.status = $3)
                            AND tee.prover_taken_at < NOW() - $4::INTERVAL
                        )
                    )
                FETCH FIRST ROW ONLY
            )
            
            INSERT INTO
            tee_proof_generation_details (
                l1_batch_number, tee_type, status, created_at, updated_at, prover_taken_at
            )
            SELECT
                l1_batch_number,
                $1,
                $2,
                NOW(),
                NOW(),
                NOW()
            FROM
                upsert
            ON CONFLICT (l1_batch_number, tee_type) DO
            UPDATE
            SET
            status = $2,
            updated_at = NOW(),
            prover_taken_at = NOW()
            RETURNING
            l1_batch_number,
            created_at
            "#,
            tee_type.to_string(),
            TeeProofGenerationJobStatus::PickedByProver.to_string(),
            TeeProofGenerationJobStatus::Failed.to_string(),
            processing_timeout,
            min_batch_number
        )
        .instrument("lock_batch_for_proving")
        .with_arg("tee_type", &tee_type)
        .with_arg("processing_timeout", &processing_timeout)
        .with_arg("l1_batch_number", &min_batch_number)
        .fetch_optional(self.storage)
        .await?
        .map(Into::into);

        Ok(locked_batch)
    }

    pub async fn unlock_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
        tee_type: TeeType,
        status: TeeProofGenerationJobStatus,
    ) -> DalResult<()> {
        let batch_number = i64::from(l1_batch_number.0);
        sqlx::query!(
            r#"
            UPDATE tee_proof_generation_details
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
                AND tee_type = $3
            "#,
            status.to_string(),
            batch_number,
            tee_type.to_string()
        )
        .instrument("unlock_batch")
        .with_arg("l1_batch_number", &batch_number)
        .with_arg("tee_type", &tee_type)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn save_proof_artifacts_metadata(
        &mut self,
        batch_number: L1BatchNumber,
        tee_type: TeeType,
        pubkey: &[u8],
        signature: &[u8],
        proof: &[u8],
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE tee_proof_generation_details
            SET
                tee_type = $1,
                status = $2,
                pubkey = $3,
                signature = $4,
                proof = $5,
                updated_at = NOW()
            WHERE
                l1_batch_number = $6
            "#,
            tee_type.to_string(),
            TeeProofGenerationJobStatus::Generated.to_string(),
            pubkey,
            signature,
            proof,
            batch_number
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("tee_type", &tee_type)
            .with_arg("pubkey", &pubkey)
            .with_arg("signature", &signature)
            .with_arg("proof", &proof)
            .with_arg("l1_batch_number", &batch_number);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Updating TEE proof for a non-existent batch number {} is not allowed",
                batch_number
            ));
            return Err(err);
        }

        Ok(())
    }

    pub async fn save_attestation(&mut self, pubkey: &[u8], attestation: &[u8]) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            INSERT INTO
            tee_attestations (pubkey, attestation)
            VALUES
            ($1, $2)
            ON CONFLICT (pubkey) DO NOTHING
            "#,
            pubkey,
            attestation
        );
        let instrumentation = Instrumented::new("save_attestation")
            .with_arg("pubkey", &pubkey)
            .with_arg("attestation", &attestation);
        instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;

        Ok(())
    }

    pub async fn get_tee_proofs(
        &mut self,
        batch_number: L1BatchNumber,
        tee_type: Option<TeeType>,
    ) -> DalResult<Vec<StorageTeeProof>> {
        let query = format!(
            r#"
            SELECT
                tp.pubkey,
                tp.signature,
                tp.proof,
                tp.updated_at,
                ta.attestation
            FROM
                tee_proof_generation_details tp
            LEFT JOIN
                tee_attestations ta ON tp.pubkey = ta.pubkey
            WHERE
                tp.l1_batch_number = $1
                AND tp.status = $2
                {}
            ORDER BY tp.l1_batch_number ASC, tp.tee_type ASC
            "#,
            tee_type.map_or_else(String::new, |_| "AND tp.tee_type = $3".to_string())
        );

        let mut query = sqlx::query_as(&query)
            .bind(i64::from(batch_number.0))
            .bind(TeeProofGenerationJobStatus::Generated.to_string());

        if let Some(tee_type) = tee_type {
            query = query.bind(tee_type.to_string());
        }

        let proofs: Vec<StorageTeeProof> = query.fetch_all(self.storage.conn()).await.unwrap();

        Ok(proofs)
    }

    /// For testing purposes only.
    pub async fn insert_tee_proof_generation_job(
        &mut self,
        batch_number: L1BatchNumber,
        tee_type: TeeType,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            INSERT INTO
            tee_proof_generation_details (
                l1_batch_number, tee_type, status, created_at, updated_at
            )
            VALUES
            ($1, $2, $3, NOW(), NOW())
            ON CONFLICT (l1_batch_number, tee_type) DO NOTHING
            "#,
            batch_number,
            tee_type.to_string(),
            TeeProofGenerationJobStatus::PickedByProver.to_string(),
        );
        let instrumentation = Instrumented::new("insert_tee_proof_generation_job")
            .with_arg("l1_batch_number", &batch_number)
            .with_arg("tee_type", &tee_type);
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
                tee_proof_generation_details AS proofs
            WHERE
                proofs.status = $1
            ORDER BY
                proofs.l1_batch_number ASC
            LIMIT
                1
            "#,
            TeeProofGenerationJobStatus::PickedByProver.to_string(),
        );
        let batch_number = Instrumented::new("get_oldest_unpicked_batch")
            .with(query)
            .fetch_optional(self.storage)
            .await?
            .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(batch_number)
    }
}
