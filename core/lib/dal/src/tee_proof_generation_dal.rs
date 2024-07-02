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
pub struct TeeProofGenerationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, EnumString, Display)]
enum TeeProofGenerationJobStatus {
    #[strum(serialize = "ready_to_be_proven")]
    ReadyToBeProven,
    #[strum(serialize = "picked_by_prover")]
    PickedByProver,
    #[strum(serialize = "generated")]
    Generated,
    #[strum(serialize = "skipped")]
    Skipped,
}

#[derive(Debug, EnumString, Display)]
pub enum TeeType {
    #[strum(serialize = "sgx")]
    Sgx,
}

impl TeeProofGenerationDal<'_, '_> {
    pub async fn get_next_block_to_be_proven(
        &mut self,
        processing_timeout: Duration,
    ) -> DalResult<Option<L1BatchNumber>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            UPDATE tee_proof_generation_details
            SET
                status = 'picked_by_prover',
                updated_at = NOW(),
                prover_taken_at = NOW()
            WHERE
                l1_batch_number = (
                    SELECT
                        proofs.l1_batch_number
                    FROM
                        tee_proof_generation_details AS proofs
                        JOIN tee_verifier_input_producer_jobs AS inputs ON proofs.l1_batch_number = inputs.l1_batch_number
                    WHERE
                        inputs.status = 'Successful'
                        AND (
                            proofs.status = 'ready_to_be_proven'
                            OR (
                                proofs.status = 'picked_by_prover'
                                AND proofs.prover_taken_at < NOW() - $1::INTERVAL
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
                tee_proof_generation_details.l1_batch_number
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
        block_number: L1BatchNumber,
        signature: &[u8],
        pubkey: &[u8],
        proof: &[u8],
        tee_type: TeeType,
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            UPDATE tee_proof_generation_details
            SET
                status = 'generated',
                signature = $1,
                pubkey = $2,
                proof = $3,
                tee_type = $4,
                updated_at = NOW()
            WHERE
                l1_batch_number = $5
            "#,
            signature,
            pubkey,
            proof,
            tee_type.to_string(),
            i64::from(block_number.0)
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("signature", &signature)
            .with_arg("pubkey", &pubkey)
            .with_arg("proof", &proof)
            .with_arg("tee_type", &tee_type);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Updating TEE proof for a non-existent batch number is not allowed"
            ));
            return Err(err);
        }

        Ok(())
    }

    pub async fn insert_tee_proof_generation_job(
        &mut self,
        block_number: L1BatchNumber,
    ) -> DalResult<()> {
        let block_number = i64::from(block_number.0);
        sqlx::query!(
            r#"
            INSERT INTO
                tee_proof_generation_details (l1_batch_number, status, created_at, updated_at)
            VALUES
                ($1, 'ready_to_be_proven', NOW(), NOW())
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            block_number,
        )
        .instrument("create_tee_proof_generation_details")
        .with_arg("l1_batch_number", &block_number)
        .report_latency()
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_oldest_unpicked_batch(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let result: Option<L1BatchNumber> = sqlx::query!(
            r#"
            SELECT
                proofs.l1_batch_number
            FROM
                tee_proof_generation_details AS proofs
                JOIN tee_verifier_input_producer_jobs AS inputs ON proofs.l1_batch_number = inputs.l1_batch_number
            WHERE
                inputs.status = 'Successful'
                AND proofs.status = 'ready_to_be_proven'
            ORDER BY
                proofs.l1_batch_number ASC
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
                "Unable to insert TEE attestation: given pubkey already has an attestation assigned"
            ));
            return Err(err);
        }

        Ok(())
    }
}
