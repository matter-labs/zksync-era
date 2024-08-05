#![doc = include_str!("../doc/TeeProofGenerationDal.md")]
use std::time::Duration;

use zksync_db_connection::{
    connection::Connection, error::DalResult, instrument::Instrumented,
    utils::pg_interval_from_duration,
};
use zksync_types::{tee_types::TeeType, L1BatchNumber};

use crate::{
    models::storage_tee_proof::StorageTeeProof,
    tee_verifier_input_producer_dal::TeeVerifierInputProducerJobStatus, Core,
};

#[derive(Debug)]
pub struct TeeProofGenerationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "tee_proofs_job_status")]
enum TeeProofGenerationJobStatus {
    ReadyToBeProven,
    PickedByProver,
    Generated,
}

impl TeeProofGenerationDal<'_, '_> {
    pub async fn get_next_batch_to_be_proven(
        &mut self,
        tee_type: TeeType,
        processing_timeout: Duration,
    ) -> DalResult<Option<L1BatchNumber>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let query = sqlx::query!(
            r#"
            UPDATE tee_proofs
            SET
                status = $1,
                updated_at = NOW(),
                prover_taken_at = NOW()
            WHERE
                tee_type = $2
                AND l1_batch_number = (
                    SELECT
                        proofs.l1_batch_number
                    FROM
                        tee_proofs AS proofs
                        JOIN tee_verifier_input_producer_jobs AS inputs ON proofs.l1_batch_number = inputs.l1_batch_number
                    WHERE
                        inputs.status = $3
                        AND (
                            proofs.status = $4
                            OR (
                                proofs.status = $5
                                AND proofs.prover_taken_at < NOW() - $6::INTERVAL
                            )
                        )
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                tee_proofs.l1_batch_number
            "#,
            TeeProofGenerationJobStatus::PickedByProver as TeeProofGenerationJobStatus,
            &tee_type.to_string(),
            TeeVerifierInputProducerJobStatus::Successful as TeeVerifierInputProducerJobStatus,
            TeeProofGenerationJobStatus::ReadyToBeProven as TeeProofGenerationJobStatus,
            TeeProofGenerationJobStatus::PickedByProver as TeeProofGenerationJobStatus,
            &processing_timeout,
        );
        let batch_number = Instrumented::new("get_next_batch_to_be_proven")
            .with_arg("tee_type", &tee_type)
            .with_arg("processing_timeout", &processing_timeout)
            .with(query)
            .fetch_optional(self.storage)
            .await?
            .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(batch_number)
    }

    pub async fn save_proof_artifacts_metadata(
        &mut self,
        batch_number: L1BatchNumber,
        tee_type: TeeType,
        pubkey: &[u8],
        signature: &[u8],
        proof: &[u8],
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            UPDATE tee_proofs
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
            TeeProofGenerationJobStatus::Generated as TeeProofGenerationJobStatus,
            pubkey,
            signature,
            proof,
            i64::from(batch_number.0)
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("tee_type", &tee_type)
            .with_arg("pubkey", &pubkey)
            .with_arg("signature", &signature)
            .with_arg("proof", &proof);
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

    pub async fn insert_tee_proof_generation_job(
        &mut self,
        batch_number: L1BatchNumber,
        tee_type: TeeType,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            INSERT INTO
                tee_proofs (l1_batch_number, tee_type, status, created_at, updated_at)
            VALUES
                ($1, $2, $3, NOW(), NOW())
            "#,
            batch_number,
            tee_type.to_string(),
            TeeProofGenerationJobStatus::ReadyToBeProven as TeeProofGenerationJobStatus,
        );
        let instrumentation = Instrumented::new("insert_tee_proof_generation_job")
            .with_arg("l1_batch_number", &batch_number)
            .with_arg("tee_type", &tee_type);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Unable to insert TEE proof for batch number {}, TEE type {}",
                batch_number,
                tee_type
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
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Unable to insert TEE attestation: pubkey {:?} already has an attestation assigned",
                pubkey
            ));
            return Err(err);
        }

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
                tee_proofs tp
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
            .bind(TeeProofGenerationJobStatus::Generated as TeeProofGenerationJobStatus);

        if let Some(tee_type) = tee_type {
            query = query.bind(tee_type.to_string());
        }

        let proofs: Vec<StorageTeeProof> = query.fetch_all(self.storage.conn()).await.unwrap();

        Ok(proofs)
    }

    pub async fn get_oldest_unpicked_batch(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let query = sqlx::query!(
            r#"
            SELECT
                proofs.l1_batch_number
            FROM
                tee_proofs AS proofs
                JOIN tee_verifier_input_producer_jobs AS inputs ON proofs.l1_batch_number = inputs.l1_batch_number
            WHERE
                inputs.status = $1
                AND proofs.status = $2
            ORDER BY
                proofs.l1_batch_number ASC
            LIMIT
                1
            "#,
            TeeVerifierInputProducerJobStatus::Successful as TeeVerifierInputProducerJobStatus,
            TeeProofGenerationJobStatus::ReadyToBeProven as TeeProofGenerationJobStatus,
        );
        let batch_number = Instrumented::new("get_oldest_unpicked_batch")
            .with(query)
            .fetch_optional(self.storage)
            .await?
            .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(batch_number)
    }
}
