#![doc = include_str!("../doc/TeeProofGenerationDal.md")]
use std::time::Duration;

use chrono::{DateTime, Utc};
use strum::{Display, EnumString};
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
    interpolate_query, match_query_as,
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
    /// The batch has been picked by a TEE prover and is currently being processed.
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
    /// The batch will not be processed again because the proof generation has been failing for an
    /// extended period, as specified in the configuration.
    #[strum(serialize = "permanently_ignored")]
    PermanentlyIgnored,
}

/// Represents a locked batch picked by a TEE prover. A batch is locked when taken by a TEE prover
/// ([TeeProofGenerationJobStatus::PickedByProver]). It can transition to one of three states:
/// 1. [TeeProofGenerationJobStatus::Generated].
/// 2. [TeeProofGenerationJobStatus::Failed].
/// 3. [TeeProofGenerationJobStatus::PermanentlyIgnored].
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
        let mut transaction = self.storage.start_transaction().await?;

        // Lock the entire tee_proof_generation_details table in EXCLUSIVE mode to prevent race
        // conditions. Locking the table ensures that two different TEE prover instances will not
        // try to prove the same batch.
        sqlx::query("LOCK TABLE tee_proof_generation_details IN EXCLUSIVE MODE")
            .instrument("lock_batch_for_proving#lock_table")
            .execute(&mut transaction)
            .await?;

        // The tee_proof_generation_details table does not have corresponding entries yet if this is
        // the first time the query is invoked for a batch.
        let batch_number = sqlx::query!(
            r#"
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
            LIMIT 1
            "#,
            tee_type.to_string(),
            TeeProofGenerationJobStatus::PickedByProver.to_string(),
            TeeProofGenerationJobStatus::Failed.to_string(),
            processing_timeout,
            min_batch_number
        )
        .instrument("lock_batch_for_proving#get_batch_no")
        .with_arg("tee_type", &tee_type)
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
            tee_proof_generation_details (
                l1_batch_number, tee_type, status, created_at, updated_at, prover_taken_at
            )
            VALUES
            (
                $1,
                $2,
                $3,
                NOW(),
                NOW(),
                NOW()
            )
            ON CONFLICT (l1_batch_number, tee_type) DO
            UPDATE
            SET
            status = $3,
            updated_at = NOW(),
            prover_taken_at = NOW()
            RETURNING
            l1_batch_number,
            created_at
            "#,
            batch_number,
            tee_type.to_string(),
            TeeProofGenerationJobStatus::PickedByProver.to_string(),
        )
        .instrument("lock_batch_for_proving#insert")
        .with_arg("batch_number", &batch_number)
        .with_arg("tee_type", &tee_type)
        .fetch_optional(&mut transaction)
        .await?
        .map(Into::into);

        transaction.commit().await?;
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
        calldata: &[u8],
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE tee_proof_generation_details
            SET
                status = $2,
                pubkey = $3,
                signature = $4,
                proof = $5,
                calldata = $6,
                updated_at = NOW(),
                eth_tx_id = NULL
            WHERE
                l1_batch_number = $7
                AND tee_type = $1
            "#,
            tee_type.to_string(),
            TeeProofGenerationJobStatus::Generated.to_string(),
            pubkey,
            signature,
            proof,
            calldata,
            batch_number
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("tee_type", &tee_type)
            .with_arg("pubkey", &pubkey)
            .with_arg("signature", &signature)
            .with_arg("proof", &proof)
            .with_arg("calldata", &calldata)
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

    pub async fn get_tee_proofs_for_eth_sender(&mut self) -> DalResult<Vec<(Vec<u8>, Vec<u8>)>> {
        let proofs = sqlx::query!(
            r#"
            SELECT signature, calldata
            FROM tee_proof_generation_details
            WHERE
                eth_tx_id IS NULL
                AND calldata IS NOT NULL
            "#,
        )
        .instrument("get_tee_proofs_for_eth_sender")
        .fetch_all(self.storage)
        .await?;

        Ok(proofs
            .into_iter()
            .filter_map(|row| match (row.signature, row.calldata) {
                (Some(signature), Some(calldata)) => Some((signature, calldata)),
                _ => None,
            })
            .collect())
    }

    pub async fn set_eth_tx_id_for_proof(
        &mut self,
        signature: &[u8],
        eth_tx_id: u32,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_proof_generation_details
            SET eth_tx_id = $1
            WHERE signature = $2
            "#,
            i32::try_from(eth_tx_id).unwrap(),
            signature,
        )
        .instrument("set_eth_tx_id_for_proof")
        .with_arg("signature", &signature)
        .with_arg("eth_tx_id", &eth_tx_id)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn save_attestation(
        &mut self,
        pubkey: &[u8],
        attestation: &[u8],
        calldata: &[u8],
    ) -> DalResult<()> {
        let query = sqlx::query!(
            r#"
            INSERT INTO
            tee_attestations (pubkey, attestation, calldata, created_at)
            VALUES
            ($1, $2, $3, NOW())
            ON CONFLICT (pubkey) DO NOTHING
            "#,
            pubkey,
            attestation,
            calldata
        );
        let instrumentation = Instrumented::new("save_attestation")
            .with_arg("pubkey", &pubkey)
            .with_arg("attestation", &attestation)
            .with_arg("calldata", &calldata);
        instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;

        Ok(())
    }

    /// Gets all attestation quote records that need to be sent to Ethereum
    pub async fn get_pending_attestations_for_eth_tx(
        &mut self,
    ) -> DalResult<Vec<(Vec<u8>, Vec<u8>)>> {
        let attestations = sqlx::query!(
            r#"
            SELECT pubkey, calldata
            FROM tee_attestations
            WHERE eth_tx_id IS NULL AND calldata IS NOT NULL AND pubkey IS NOT NULL
            "#
        )
        .instrument("get_pending_attestations_for_eth_tx")
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        Ok(attestations
            .into_iter()
            .filter_map(|row| match (row.pubkey, row.calldata) {
                (pubkey, Some(calldata)) => Some((pubkey, calldata)),
                _ => None,
            })
            .collect())
    }

    /// Set eth transaction id for pubkey register
    pub async fn set_eth_tx_id_for_attestation(
        &mut self,
        pubkey: &[u8],
        eth_tx_id: u32,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE tee_attestations
            SET eth_tx_id = $1
            WHERE pubkey = $2
            "#,
            i32::try_from(eth_tx_id).unwrap(),
            pubkey,
        )
        .instrument("set_eth_tx_id_for_attestation")
        .report_latency()
        .with_arg("pubkey", &pubkey)
        .with_arg("eth_tx_id", &eth_tx_id)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_tee_proofs(
        &mut self,
        batch_number: L1BatchNumber,
        tee_type: Option<TeeType>,
    ) -> DalResult<Vec<StorageTeeProof>> {
        let query = match_query_as!(
            StorageTeeProof,
            [
            r#"
            SELECT
                tp.pubkey,
                tp.signature,
                tp.proof,
                tp.updated_at,
                tp.status,
                ta.attestation
            FROM
                tee_proof_generation_details tp
            LEFT JOIN
                tee_attestations ta ON tp.pubkey = ta.pubkey
            WHERE
                tp.l1_batch_number = $1
            "#,
            _,
            "ORDER BY tp.l1_batch_number ASC, tp.tee_type ASC"
            ],
            match(&tee_type) {
                Some(tee_type) =>
                    ("AND tp.tee_type = $2"; i64::from(batch_number.0), tee_type.to_string()),
                None => (""; i64::from(batch_number.0)),
            }
        );

        let proofs = query
            .instrument("get_tee_proofs")
            .with_arg("l1_batch_number", &batch_number)
            .fetch_all(self.storage)
            .await?;

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
