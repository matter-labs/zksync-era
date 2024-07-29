use std::time::Duration;

use zksync_db_connection::{
    connection::Connection, error::DalResult, instrument::Instrumented,
    utils::pg_interval_from_duration,
};
use zksync_types::{tee_types::TeeType, L1BatchNumber};

use crate::{
    models::storage_tee_proof::{StorageTeeProof, TmpStorageTeeProof},
    Core,
};

#[derive(Debug)]
pub struct TeeProofGenerationDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl TeeProofGenerationDal<'_, '_> {
    pub async fn get_next_batch_to_be_proven(
        &mut self,
        tee_type: TeeType,
        processing_timeout: Duration,
    ) -> DalResult<Option<(i64, L1BatchNumber)>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let query = sqlx::query!(
            r#"
            UPDATE tee_proofs
            SET
                prover_taken_at = NOW()
            WHERE
                id = (
                    SELECT
                        proofs.id
                    FROM
                        tee_proofs AS proofs
                        JOIN tee_verifier_input_producer_jobs AS inputs ON proofs.l1_batch_number = inputs.l1_batch_number
                    WHERE
                        inputs.status = 'Successful'
                        AND proofs.tee_type = $1
                        AND (
                            proofs.prover_taken_at IS NULL
                            OR proofs.prover_taken_at < NOW() - $2::INTERVAL
                        )
                    ORDER BY
                        tee_proofs.l1_batch_number ASC,
                        proofs.prover_taken_at ASC NULLS FIRST
                    LIMIT
                        1
                    FOR UPDATE
                        SKIP LOCKED
                )
            RETURNING
                tee_proofs.id,
                tee_proofs.l1_batch_number
            "#,
            &tee_type.to_string(),
            &processing_timeout,
        );
        let batch_number = Instrumented::new("get_next_batch_to_be_proven")
            .with_arg("tee_type", &tee_type)
            .with_arg("processing_timeout", &processing_timeout)
            .with(query)
            .fetch_optional(self.storage)
            .await?
            .map(|row| (row.id as i64, L1BatchNumber(row.l1_batch_number as u32)));

        Ok(batch_number)
    }

    pub async fn save_proof_artifacts_metadata(
        &mut self,
        proof_id: i64,
        pubkey: &[u8],
        signature: &[u8],
        proof: &[u8],
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            UPDATE tee_proofs
            SET
                pubkey = $1,
                signature = $2,
                proof = $3,
                proved_at = NOW()
            WHERE
                id = $4
            "#,
            pubkey,
            signature,
            proof,
            proof_id
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("pubkey", &pubkey)
            .with_arg("signature", &signature)
            .with_arg("proof", &proof)
            .with_arg("proof_id", &proof_id);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Updating TEE proof for a non-existent ID {} is not allowed",
                proof_id
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
                tee_proofs (l1_batch_number, tee_type, created_at)
            VALUES
                ($1, $2, NOW())
            "#,
            batch_number,
            tee_type.to_string(),
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
                "Unable to insert TEE proof for {}, {}",
                batch_number,
                tee_type
            ));
            return Err(err);
        }

        Ok(())
    }

    #[cfg(test)]
    pub async fn get_oldest_unpicked_batch(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let query = sqlx::query!(
            r#"
            SELECT
                proofs.l1_batch_number
            FROM
                tee_proofs AS proofs
                JOIN tee_verifier_input_producer_jobs AS inputs ON proofs.l1_batch_number = inputs.l1_batch_number
            WHERE
                inputs.status = 'Successful'
                AND proofs.prover_taken_at IS NULL
            ORDER BY
                proofs.l1_batch_number ASC
            LIMIT
                1
            "#,
        );
        let batch_number = Instrumented::new("get_oldest_unpicked_batch")
            .with(query)
            .fetch_optional(self.storage)
            .await?
            .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(batch_number)
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

    pub async fn get_tee_proofs(
        &mut self,
        batch_number: L1BatchNumber,
        tee_type: Option<TeeType>,
    ) -> DalResult<Vec<StorageTeeProof>> {
        let query = format!(
            r#"
            SELECT
                tp.id,
                tp.pubkey,
                tp.signature,
                tp.proof,
                tp.proved_at,
                ta.attestation
            FROM
                tee_proofs tp
            LEFT JOIN
                tee_attestations ta ON tp.pubkey = ta.pubkey
            WHERE
                tp.l1_batch_number = {}
                {}
            ORDER BY tp.id
            "#,
            i64::from(batch_number.0),
            tee_type.map_or_else(String::new, |tt| format!("AND tp.tee_type = '{}'", tt))
        );

        let proofs: Vec<StorageTeeProof> = sqlx::query_as(&query)
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row: TmpStorageTeeProof| StorageTeeProof {
                l1_batch_number: batch_number,
                tee_type,
                pubkey: row.pubkey,
                signature: row.signature,
                proof: row.proof,
                attestation: row.attestation,
                proved_at: row.proved_at,
            })
            .collect();

        Ok(proofs)
    }
}
