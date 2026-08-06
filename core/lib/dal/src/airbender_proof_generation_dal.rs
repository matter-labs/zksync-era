#![doc = include_str!("../doc/AirbenderProofGenerationDal.md")]
use std::{collections::HashSet, time::Duration};

use chrono::{DateTime, Utc};
use strum::{Display, EnumString};
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
    utils::pg_interval_from_duration,
};
use zksync_types::{protocol_version::ProtocolSemanticVersion, L1BatchNumber, H256};

use crate::{
    models::{
        parse_protocol_version,
        storage_airbender_proof::{
            StorageAirbenderProof, StorageAirbenderSnarkProof, StorageLockedBatch,
        },
    },
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
    /// The FRI proof has been successfully generated and submitted for the batch.
    #[strum(serialize = "generated")]
    Generated,
    /// The batch has been picked by a SNARK prover, which is wrapping the FRI proof.
    #[strum(serialize = "picked_for_snark")]
    PickedForSnark,
    /// The SNARK proof has been generated and submitted for the batch and is ready for L1.
    #[strum(serialize = "snark_generated")]
    SnarkGenerated,
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
    /// The protocol version of the batch.
    pub protocol_version: ProtocolSemanticVersion,
    /// The creation time of the job for this batch. It is used to determine if the batch should
    /// transition to [AirbenderProofGenerationJobStatus::Failed].
    pub created_at: DateTime<Utc>,
}

impl AirbenderProofGenerationDal<'_, '_> {
    /// Locks the next batch for Airbender FRI proving. A prover is identified by the Airbender
    /// SNARK-wrapper VK hash it carries; the recorded version is the batch's own minor with the
    /// highest patch of that minor registered for that key.
    ///
    /// Recorded versions must never decrease in batch number, compared as `(minor, patch)`:
    /// `eth_sender` submits strictly in order against the single key L1 holds, so a batch proven
    /// with a superseded key blocks every batch behind it. Step 2 enforces that against the
    /// database, with no process-local state.
    ///
    /// Reclaims (Step 1) keep the version recorded at first lock and only go to provers whose key
    /// matches it, so an old generation can always finish its own batches. Operational corollary:
    /// retire a prover generation only once its batches reach `snark_generated`.
    pub async fn lock_batch_for_proving(
        &mut self,
        processing_timeout: Duration,
        min_batch_number: L1BatchNumber,
        max_attempts: u32,
        airbender_vk_hash: H256,
    ) -> DalResult<Option<LockedBatch>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let min_batch_number = i64::from(min_batch_number.0);
        let max_attempts = i16::try_from(max_attempts).unwrap_or(i16::MAX);
        let picked = AirbenderProofGenerationJobStatus::PickedByProver.to_string();
        let failed = AirbenderProofGenerationJobStatus::Failed.to_string();

        // Step 1: reclaim a timed-out or failed batch. Each reclaim bumps `attempts`, so a batch
        // that keeps failing is eventually abandoned rather than retried forever. Only batches
        // whose recorded version carries the requesting VK are handed out: that version determines
        // the blob key and the key L1 verifies against. SKIP LOCKED keeps provers off each other.
        let locked_batch = sqlx::query_as!(
            StorageLockedBatch,
            r#"
            UPDATE airbender_proof_generation_details
            SET
                status = $1,
                updated_at = NOW(),
                prover_taken_at = NOW(),
                attempts = attempts + 1
            WHERE
                l1_batch_number = (
                    SELECT apgd.l1_batch_number
                    FROM airbender_proof_generation_details apgd
                    JOIN proof_generation_details p
                        ON p.l1_batch_number = apgd.l1_batch_number
                    WHERE
                        p.l1_batch_number >= $3
                        AND p.vm_run_data_blob_url IS NOT NULL
                        AND p.proof_gen_data_blob_url IS NOT NULL
                        AND apgd.attempts < $5
                        AND (
                            apgd.status = $2
                            OR (
                                apgd.status = $1
                                AND apgd.prover_taken_at < NOW() - $4::INTERVAL
                            )
                        )
                        AND EXISTS (
                            SELECT 1 FROM protocol_patches pp
                            WHERE
                                pp.minor = apgd.protocol_version
                                AND pp.patch = apgd.protocol_version_patch
                                AND pp.airbender_snark_wrapper_vk_hash = $6
                        )
                    ORDER BY apgd.l1_batch_number ASC
                    LIMIT 1
                    FOR UPDATE OF apgd SKIP LOCKED
                )
            RETURNING l1_batch_number,
            created_at,
            protocol_version AS "protocol_version!",
            protocol_version_patch
            "#,
            picked,
            failed,
            min_batch_number,
            processing_timeout,
            max_attempts,
            airbender_vk_hash.as_bytes(),
        )
        .instrument("lock_batch_for_proving#reclaim")
        .with_arg("processing_timeout", &processing_timeout)
        .with_arg("min_batch_number", &min_batch_number)
        .with_arg("max_attempts", &max_attempts)
        .with_arg("airbender_vk_hash", &airbender_vk_hash)
        .fetch_optional(&mut *self.storage)
        .await?
        .map(Into::into);

        if locked_batch.is_some() {
            return Ok(locked_batch);
        }

        // Step 2: claim the next batch in line. One statement, so the candidate, the version floor
        // and the prover's patch all come from one snapshot.
        //
        // The candidate (`l.number = ...`) is one past the highest existing assignment, or — before
        // anything has been assigned — the oldest surviving batch at or above `min_batch_number`
        // (not `min_batch_number` itself, which a pruned node would aim under forever).
        // Deliberately not "the lowest batch without an assignment": that walks backwards into an
        // old gap when `first_processed_batch` is lowered, so lowering it is a no-op instead.
        //
        // `version_floor` is the highest `(minor, patch)` already assigned, `(0, 0)` if none. The
        // insert is refused below it.
        //
        // Why two handler processes cannot commit a decreasing sequence: assignments are only
        // created at `MAX + 1`, so the committed set is a contiguous run. A claim committed before
        // another's snapshot is visible to it in both the candidate and the floor; one still
        // uncommitted is invisible, so both resolve the same candidate and collide on the primary
        // key (`ON CONFLICT DO NOTHING` waits out the other, then yields no job — or takes the
        // batch if it rolled back, since an uncommitted version constrains nothing). Either way the
        // winner of batch `m + 1` saw the whole committed prefix, so by induction versions never
        // decrease.
        //
        // That rests on `min_batch_number` agreeing across the fleet: *raising*
        // `first_processed_batch` deliberately leaves a gap, so roll it out on its own rather than
        // alongside a key rotation.
        //
        // A VK with no patch for the batch's minor yields no row from the `JOIN LATERAL`, which
        // also covers a NULL `protocol_version`.
        let locked_batch = sqlx::query_as!(
            StorageLockedBatch,
            r#"
            INSERT INTO airbender_proof_generation_details (
                l1_batch_number, status, created_at, updated_at, prover_taken_at,
                attempts, protocol_version, protocol_version_patch
            )
            SELECT
                l.number,
                $1,
                NOW(),
                NOW(),
                NOW(),
                1,
                l.protocol_version,
                prover_patch.patch
            FROM l1_batches l
            JOIN proof_generation_details p ON p.l1_batch_number = l.number
            JOIN LATERAL (
                SELECT pp.patch
                FROM protocol_patches pp
                WHERE
                    pp.minor = l.protocol_version
                    AND pp.airbender_snark_wrapper_vk_hash = $3
                ORDER BY pp.patch DESC
                LIMIT 1
            ) prover_patch ON TRUE
            LEFT JOIN LATERAL (
                SELECT
                    apgd.protocol_version AS minor,
                    apgd.protocol_version_patch AS patch
                FROM airbender_proof_generation_details apgd
                WHERE apgd.protocol_version IS NOT NULL
                ORDER BY apgd.protocol_version DESC, apgd.protocol_version_patch DESC
                LIMIT 1
            ) version_floor ON TRUE
            WHERE
                l.number = COALESCE(
                    (
                        SELECT GREATEST(assigned.highest + 1, $2, 1::BIGINT)
                        FROM (
                            SELECT MAX(l1_batch_number) AS highest
                            FROM airbender_proof_generation_details
                        ) assigned
                        WHERE assigned.highest IS NOT NULL
                    ),
                    (
                        SELECT MIN(number)
                        FROM l1_batches
                        WHERE number >= GREATEST($2, 1::BIGINT)
                    )
                )
                AND p.vm_run_data_blob_url IS NOT NULL
                AND p.proof_gen_data_blob_url IS NOT NULL
                AND (l.protocol_version, prover_patch.patch)
                >= (COALESCE(version_floor.minor, 0), COALESCE(version_floor.patch, 0))
            ON CONFLICT (l1_batch_number) DO NOTHING
            RETURNING l1_batch_number,
            created_at,
            protocol_version AS "protocol_version!",
            protocol_version_patch
            "#,
            picked,
            min_batch_number,
            airbender_vk_hash.as_bytes(),
        )
        .instrument("lock_batch_for_proving#new")
        .with_arg("min_batch_number", &min_batch_number)
        .with_arg("airbender_vk_hash", &airbender_vk_hash)
        .fetch_optional(&mut *self.storage)
        .await?
        .map(Into::into);

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
        prover_id: &str,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE airbender_proof_generation_details
            SET
                status = $1,
                proof_blob_url = $2,
                prover_id = $3,
                updated_at = NOW()
            WHERE
                l1_batch_number = $4
                AND status = $5
            "#,
            AirbenderProofGenerationJobStatus::Generated.to_string(),
            proof_blob_url,
            prover_id,
            batch_number,
            AirbenderProofGenerationJobStatus::PickedByProver.to_string(),
        );
        let instrumentation = Instrumented::new("save_proof_artifacts_metadata")
            .with_arg("proof_blob_url", &proof_blob_url)
            .with_arg("prover_id", &prover_id)
            .with_arg("l1_batch_number", &batch_number);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot save proof for batch {}: batch is not in '{}' status (it may have timed out and been reassigned)",
                batch_number,
                AirbenderProofGenerationJobStatus::PickedByProver,
            ));
            return Err(err);
        }

        Ok(())
    }

    /// Marks a FRI proving job as failed after a prover reports it could not produce the proof.
    /// The batch goes back to `failed` and is retried by [`Self::lock_batch_for_proving`] until the
    /// attempts limit is hit. Only a batch currently `picked_by_prover` is affected, so a stale
    /// prover can't fail a batch that already timed out and was reassigned.
    pub async fn mark_proof_failed(
        &mut self,
        batch_number: L1BatchNumber,
        error: &str,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE airbender_proof_generation_details
            SET
                status = $1,
                error = $2,
                updated_at = NOW()
            WHERE
                l1_batch_number = $3
                AND status = $4
            "#,
            AirbenderProofGenerationJobStatus::Failed.to_string(),
            error,
            batch_number,
            AirbenderProofGenerationJobStatus::PickedByProver.to_string(),
        );
        let instrumentation = Instrumented::new("mark_proof_failed")
            .with_arg("l1_batch_number", &batch_number)
            .with_arg("error", &error);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot fail proof for batch {}: batch is not in '{}' status (it may have timed out and been reassigned)",
                batch_number,
                AirbenderProofGenerationJobStatus::PickedByProver,
            ));
            return Err(err);
        }

        Ok(())
    }

    /// Lock a batch for SNARK wrapping. Picks the oldest batch whose FRI proof has been
    /// submitted (`status = 'generated'`), or reclaims a `picked_for_snark` batch whose
    /// `snark_taken_at` exceeded `processing_timeout`. Only batches whose recorded proving
    /// version carries the requesting prover's Airbender SNARK-wrapper VK are handed out — the
    /// wrapper proof must verify against the key registered for that protocol version on L1.
    ///
    /// Like the reclaim path in [`Self::lock_batch_for_proving`], this is intentionally not subject
    /// to the version-monotonicity check: an already-generated FRI proof must still be wrapped under
    /// the version it was produced for, even after later batches moved to a newer one.
    pub async fn lock_batch_for_snark(
        &mut self,
        processing_timeout: Duration,
        min_batch_number: L1BatchNumber,
        max_attempts: u32,
        airbender_vk_hash: H256,
    ) -> DalResult<Option<LockedBatch>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let min_batch_number = i64::from(min_batch_number.0);
        let max_attempts = i16::try_from(max_attempts).unwrap_or(i16::MAX);
        let picked_for_snark = AirbenderProofGenerationJobStatus::PickedForSnark.to_string();
        let generated = AirbenderProofGenerationJobStatus::Generated.to_string();

        // Each SNARK pick (a fresh `generated` batch, a reverted failure, or a reclaimed timeout)
        // bumps `snark_attempts`; a batch is only picked while `snark_attempts < max_attempts`, so
        // SNARK wrapping is retried only a bounded number of times.
        let locked_batch = sqlx::query_as!(
            StorageLockedBatch,
            r#"
            UPDATE airbender_proof_generation_details
            SET
                status = $1,
                updated_at = NOW(),
                snark_taken_at = NOW(),
                snark_attempts = snark_attempts + 1
            WHERE
                l1_batch_number = (
                    SELECT apgd.l1_batch_number
                    FROM airbender_proof_generation_details apgd
                    WHERE
                        apgd.l1_batch_number >= $3
                        AND apgd.proof_blob_url IS NOT NULL
                        AND apgd.snark_attempts < $5
                        AND (
                            apgd.status = $2
                            OR (
                                apgd.status = $1
                                AND apgd.snark_taken_at < NOW() - $4::INTERVAL
                            )
                        )
                        AND EXISTS (
                            SELECT 1 FROM protocol_patches pp
                            WHERE
                                pp.minor = apgd.protocol_version
                                AND pp.patch = apgd.protocol_version_patch
                                AND pp.airbender_snark_wrapper_vk_hash = $6
                        )
                    ORDER BY apgd.l1_batch_number ASC
                    LIMIT 1
                    FOR UPDATE OF apgd SKIP LOCKED
                )
            RETURNING l1_batch_number,
            protocol_version AS "protocol_version!",
            protocol_version_patch,
            created_at
            "#,
            picked_for_snark,
            generated,
            min_batch_number,
            processing_timeout,
            max_attempts,
            airbender_vk_hash.as_bytes(),
        )
        .instrument("lock_batch_for_snark")
        .with_arg("processing_timeout", &processing_timeout)
        .with_arg("min_batch_number", &min_batch_number)
        .with_arg("max_attempts", &max_attempts)
        .with_arg("airbender_vk_hash", &airbender_vk_hash)
        .fetch_optional(self.storage)
        .await?
        .map(Into::into);

        Ok(locked_batch)
    }

    pub async fn save_snark_proof_artifacts_metadata(
        &mut self,
        batch_number: L1BatchNumber,
        snark_proof_blob_url: &str,
        snark_prover_id: &str,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE airbender_proof_generation_details
            SET
                status = $1,
                snark_proof_blob_url = $2,
                snark_prover_id = $3,
                updated_at = NOW()
            WHERE
                l1_batch_number = $4
            "#,
            AirbenderProofGenerationJobStatus::SnarkGenerated.to_string(),
            snark_proof_blob_url,
            snark_prover_id,
            batch_number,
        );
        let instrumentation = Instrumented::new("save_snark_proof_artifacts_metadata")
            .with_arg("snark_proof_blob_url", &snark_proof_blob_url)
            .with_arg("snark_prover_id", &snark_prover_id)
            .with_arg("l1_batch_number", &batch_number);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot save SNARK proof for batch {}: batch is not in '{}' or '{}' status (it may have timed out and been reassigned)",
                batch_number,
                AirbenderProofGenerationJobStatus::PickedForSnark,
                AirbenderProofGenerationJobStatus::Generated,
            ));
            return Err(err);
        }

        Ok(())
    }

    /// Marks a SNARK wrapping job as failed after a prover reports it could not produce the proof.
    /// The batch reverts to `generated` (its FRI proof is still valid) so it re-enters the SNARK
    /// queue, retried by [`Self::lock_batch_for_snark`] until the attempts limit is hit. Only a
    /// batch currently `picked_for_snark` is affected.
    pub async fn mark_snark_proof_failed(
        &mut self,
        batch_number: L1BatchNumber,
        error: &str,
    ) -> DalResult<()> {
        let batch_number = i64::from(batch_number.0);
        let query = sqlx::query!(
            r#"
            UPDATE airbender_proof_generation_details
            SET
                status = $1,
                error = $2,
                updated_at = NOW()
            WHERE
                l1_batch_number = $3
                AND status = $4
            "#,
            AirbenderProofGenerationJobStatus::Generated.to_string(),
            error,
            batch_number,
            AirbenderProofGenerationJobStatus::PickedForSnark.to_string(),
        );
        let instrumentation = Instrumented::new("mark_snark_proof_failed")
            .with_arg("l1_batch_number", &batch_number)
            .with_arg("error", &error);
        let result = instrumentation
            .clone()
            .with(query)
            .execute(self.storage)
            .await?;
        if result.rows_affected() == 0 {
            let err = instrumentation.constraint_error(anyhow::anyhow!(
                "Cannot fail SNARK proof for batch {}: batch is not in '{}' status (it may have timed out and been reassigned)",
                batch_number,
                AirbenderProofGenerationJobStatus::PickedForSnark,
            ));
            return Err(err);
        }

        Ok(())
    }

    pub async fn get_airbender_snark_proof(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Option<StorageAirbenderSnarkProof>> {
        let proof = sqlx::query_as!(
            StorageAirbenderSnarkProof,
            r#"
            SELECT
                apgd.snark_proof_blob_url,
                apgd.updated_at,
                apgd.status
            FROM
                airbender_proof_generation_details apgd
            WHERE
                apgd.l1_batch_number = $1
            "#,
            i64::from(batch_number.0)
        )
        .instrument("get_airbender_snark_proof")
        .with_arg("l1_batch_number", &batch_number)
        .fetch_optional(self.storage)
        .await?;

        Ok(proof)
    }

    pub async fn get_airbender_fri_proof(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Option<StorageAirbenderProof>> {
        let proof = sqlx::query_as!(
            StorageAirbenderProof,
            r#"
            SELECT
                apgd.proof_blob_url,
                apgd.updated_at,
                apgd.status
            FROM
                airbender_proof_generation_details apgd
            WHERE
                apgd.l1_batch_number = $1
            "#,
            i64::from(batch_number.0)
        )
        .instrument("get_airbender_fri_proof")
        .with_arg("l1_batch_number", &batch_number)
        .fetch_optional(self.storage)
        .await?;

        Ok(proof)
    }

    /// Returns the protocol semantic version the batch is being proved under, as persisted by
    /// [`Self::lock_batch_for_proving`] when the batch was locked. `None` if the batch is unknown or
    /// has no recorded version.
    pub async fn get_batch_protocol_version(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Option<ProtocolSemanticVersion>> {
        sqlx::query!(
            r#"
            SELECT
                protocol_version,
                protocol_version_patch
            FROM
                airbender_proof_generation_details
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(batch_number.0)
        )
        .try_map(|row| {
            row.protocol_version
                .map(|minor| {
                    parse_protocol_version(minor).map(|minor| ProtocolSemanticVersion {
                        minor,
                        patch: (row.protocol_version_patch as u32).into(),
                    })
                })
                .transpose()
        })
        .instrument("get_batch_protocol_version")
        .with_arg("l1_batch_number", &batch_number)
        .fetch_optional(self.storage)
        .await
        .map(Option::flatten)
    }

    /// Returns, out of `batch_numbers`, the subset whose Airbender FRI proof has already been
    /// produced (`proof_blob_url IS NOT NULL`). Used by the eth_sender to gate commits on the FRI
    /// proof being present in a single query rather than one lookup per batch.
    pub async fn get_airbender_fri_proven_batches(
        &mut self,
        batch_numbers: &[L1BatchNumber],
    ) -> DalResult<HashSet<L1BatchNumber>> {
        let numbers: Vec<i64> = batch_numbers.iter().map(|n| i64::from(n.0)).collect();
        let rows = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                airbender_proof_generation_details
            WHERE
                l1_batch_number = ANY($1)
                AND proof_blob_url IS NOT NULL
            "#,
            &numbers
        )
        .instrument("get_airbender_fri_proven_batches")
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| L1BatchNumber(row.l1_batch_number as u32))
            .collect())
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
                l1_batch_number, status, protocol_version, protocol_version_patch,
                created_at, updated_at
            )
            VALUES
            (
                $1,
                $2,
                (SELECT minor FROM protocol_patches ORDER BY minor DESC, patch DESC LIMIT 1
                ),
                COALESCE(
                    (
                        SELECT patch
                        FROM protocol_patches
                        ORDER BY minor DESC, patch DESC
                        LIMIT 1
                    ),
                    0
                ),
                NOW(),
                NOW()
            )
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
        let batch_number = Instrumented::new("get_oldest_picked_by_prover_batch")
            .with(query)
            .fetch_optional(self.storage)
            .await?
            .map(|row| L1BatchNumber(row.l1_batch_number as u32));

        Ok(batch_number)
    }

    /// Number of batches waiting for FRI proving: never started, or `failed` but still within the
    /// retry budget. A batch that exhausted `max_attempts` is permanently abandoned and excluded,
    /// so the gauge reflects work that will actually be picked up.
    pub async fn get_ready_for_proving_count(
        &mut self,
        min_batch_number: L1BatchNumber,
        max_attempts: u32,
    ) -> DalResult<i64> {
        let min_batch_number = i64::from(min_batch_number.0);
        let max_attempts = i16::try_from(max_attempts).unwrap_or(i16::MAX);
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
                    OR (apgd.status = $2 AND apgd.attempts < $3)
                )
            "#,
            min_batch_number,
            AirbenderProofGenerationJobStatus::Failed.to_string(),
            max_attempts,
        )
        .instrument("get_ready_for_proving_count")
        .with_arg("min_batch_number", &min_batch_number)
        .with_arg("max_attempts", &max_attempts)
        .fetch_one(self.storage)
        .await?;

        Ok(row.count)
    }

    /// Number of batches whose FRI proof has been submitted (`status = 'generated'`) and are
    /// waiting to be wrapped into a SNARK proof, excluding those that exhausted the SNARK retry
    /// budget (`snark_attempts >= max_attempts`).
    pub async fn get_ready_for_snark_count(
        &mut self,
        min_batch_number: L1BatchNumber,
        max_attempts: u32,
    ) -> DalResult<i64> {
        let min_batch_number = i64::from(min_batch_number.0);
        let max_attempts = i16::try_from(max_attempts).unwrap_or(i16::MAX);
        let row = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS "count!"
            FROM
                airbender_proof_generation_details apgd
            WHERE
                apgd.l1_batch_number >= $1
                AND apgd.proof_blob_url IS NOT NULL
                AND apgd.status = $2
                AND apgd.snark_attempts < $3
            "#,
            min_batch_number,
            AirbenderProofGenerationJobStatus::Generated.to_string(),
            max_attempts,
        )
        .instrument("get_ready_for_snark_count")
        .with_arg("min_batch_number", &min_batch_number)
        .with_arg("max_attempts", &max_attempts)
        .fetch_one(self.storage)
        .await?;

        Ok(row.count)
    }
}
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{
        block::L1BatchHeader,
        protocol_version::{L1VerifierConfig, ProtocolSemanticVersion, VersionPatch},
        settlement::SettlementLayer,
        L1BatchNumber, ProtocolVersionId,
    };

    use super::*;
    use crate::{ConnectionPool, CoreDal};

    /// The Airbender SNARK-wrapper VK hash the test prover identifies itself with.
    const PROVER_VK: H256 = H256::repeat_byte(0xab);
    /// A second prover generation's key, e.g. v31.2 while [`PROVER_VK`] is v31.1.
    const NEXT_GEN_VK: H256 = H256::repeat_byte(0xcd);

    async fn save_patch(conn: &mut Connection<'_, Core>, minor: ProtocolVersionId, patch: u32) {
        save_patch_with_vk(conn, minor, patch, Some(PROVER_VK)).await;
    }

    async fn save_patch_with_vk(
        conn: &mut Connection<'_, Core>,
        minor: ProtocolVersionId,
        patch: u32,
        airbender_vk: Option<H256>,
    ) {
        conn.protocol_versions_dal()
            .save_protocol_version(
                ProtocolSemanticVersion {
                    minor,
                    patch: VersionPatch(patch),
                },
                0,
                L1VerifierConfig {
                    airbender_snark_wrapper_vk_hash: airbender_vk,
                    ..L1VerifierConfig::default()
                },
                BaseSystemContractsHashes::default(),
                None,
            )
            .await
            .unwrap();
    }

    async fn insert_provable_batch(
        conn: &mut Connection<'_, Core>,
        number: L1BatchNumber,
        minor: ProtocolVersionId,
    ) {
        insert_batch_without_inputs(conn, number, minor).await;
        mark_inputs_ready(conn, number).await;
    }

    /// Inserts a batch whose proving inputs are not on GCS yet, so it is not claimable. BWIP proves
    /// several batches concurrently, so a higher batch can become claimable before a lower one.
    async fn insert_batch_without_inputs(
        conn: &mut Connection<'_, Core>,
        number: L1BatchNumber,
        minor: ProtocolVersionId,
    ) {
        let header = L1BatchHeader::new(
            number,
            100,
            BaseSystemContractsHashes::default(),
            minor,
            SettlementLayer::for_tests(),
        );
        conn.blocks_dal()
            .insert_mock_l1_batch(&header)
            .await
            .unwrap();
        conn.proof_generation_dal()
            .insert_proof_generation_details(number)
            .await
            .unwrap();
    }

    /// Marks the proving inputs of an already-inserted batch as present, making it claimable.
    async fn mark_inputs_ready(conn: &mut Connection<'_, Core>, number: L1BatchNumber) {
        conn.proof_generation_dal()
            .save_vm_runner_artifacts_metadata(number, "vm_run")
            .await
            .unwrap();
        conn.proof_generation_dal()
            .save_merkle_paths_artifacts_metadata(number, "merkle_paths")
            .await
            .unwrap();
    }

    /// Long enough that no already-picked batch counts as reclaimable. `Duration::MAX` cannot be
    /// used: `pg_interval_from_duration` overflows it into an interval that times out everything.
    const NO_RECLAIM: Duration = Duration::from_secs(600);

    /// One poll on the boundaries `AirbenderRequestProcessor::get_proof_generation_data` runs on:
    /// own pooled connection, own transaction. Nothing outlives the call, so two calls with
    /// different keys model two handler processes.
    async fn poll(pool: &ConnectionPool<Core>, vk: H256, timeout: Duration) -> Option<LockedBatch> {
        let mut conn = pool.connection().await.unwrap();
        let mut transaction = conn.start_transaction().await.unwrap();
        let locked = transaction
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(timeout, L1BatchNumber(0), 10, vk)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        locked
    }

    /// Records an assignment at an explicit version, bypassing the lock, to set up a shape the lock
    /// itself would not produce.
    async fn insert_claim_at_version(
        conn: &mut Connection<'_, Core>,
        number: L1BatchNumber,
        minor: ProtocolVersionId,
        patch: u32,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            airbender_proof_generation_details (
                l1_batch_number, status, created_at, updated_at, prover_taken_at,
                attempts, protocol_version, protocol_version_patch
            )
            VALUES
            ($1, $2, NOW(), NOW(), NOW(), 1, $3, $4)
            "#,
            i64::from(number.0),
            AirbenderProofGenerationJobStatus::PickedByProver.to_string(),
            minor as i32,
            patch as i32,
        )
        .execute(conn.conn())
        .await
        .unwrap();
    }

    /// The recorded proving version of every assignment, in batch order.
    async fn recorded_versions(conn: &mut Connection<'_, Core>) -> Vec<(i64, i32, i32)> {
        sqlx::query!(
            r#"
            SELECT
                l1_batch_number,
                protocol_version,
                protocol_version_patch
            FROM
                airbender_proof_generation_details
            ORDER BY
                l1_batch_number
            "#
        )
        .fetch_all(conn.conn())
        .await
        .unwrap()
        .into_iter()
        .filter_map(|row| {
            row.protocol_version
                .map(|minor| (row.l1_batch_number, minor, row.protocol_version_patch))
        })
        .collect()
    }

    /// Asserts the core invariant: recorded proving versions never decrease as batch numbers grow.
    async fn assert_versions_non_decreasing(conn: &mut Connection<'_, Core>) {
        let versions = recorded_versions(conn).await;
        for pair in versions.windows(2) {
            let (prev_batch, prev_minor, prev_patch) = pair[0];
            let (batch, minor, patch) = pair[1];
            assert!(
                (minor, patch) >= (prev_minor, prev_patch),
                "version regressed: batch {prev_batch} at {prev_minor}.{prev_patch} \
                 is followed by batch {batch} at {minor}.{patch}"
            );
        }
    }

    /// The recorded version is the batch's own minor with the latest patch for that minor — not the
    /// globally latest protocol version.
    #[tokio::test]
    async fn lock_records_batch_minor_with_latest_patch() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        // The batch was executed under an older minor (V30) ...
        let batch_minor = ProtocolVersionId::Version30;
        save_patch(&mut conn, batch_minor, 0).await;
        save_patch(&mut conn, batch_minor, 3).await;
        // ... while a newer minor (the global latest) also has patches registered.
        let latest_minor = ProtocolVersionId::latest();
        assert!(latest_minor > batch_minor);
        save_patch(&mut conn, latest_minor, 0).await;
        save_patch(&mut conn, latest_minor, 9).await;

        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        let locked = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("batch should be lockable");

        assert_eq!(locked.l1_batch_number, L1BatchNumber(1));
        // Batch minor, latest patch for that minor — not the global latest (V31/patch 9).
        assert_eq!(
            locked.protocol_version,
            ProtocolSemanticVersion {
                minor: batch_minor,
                patch: VersionPatch(3),
            }
        );
    }

    /// A reclaim preserves the version recorded at first lock instead of recomputing it.
    #[tokio::test]
    async fn reclaim_preserves_recorded_version() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::Version30;
        save_patch(&mut conn, batch_minor, 0).await;
        save_patch(&mut conn, batch_minor, 3).await;

        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        let first = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("batch should be lockable");

        // A newer patch appears after the batch was first locked.
        save_patch(&mut conn, batch_minor, 7).await;

        // Zero timeout makes the picked batch immediately reclaimable.
        let reclaimed = poll(&pool, PROVER_VK, Duration::ZERO)
            .await
            .expect("batch should be reclaimable");

        assert_eq!(reclaimed.l1_batch_number, L1BatchNumber(1));
        assert_eq!(reclaimed.protocol_version, first.protocol_version);
        assert_eq!(reclaimed.protocol_version.patch, VersionPatch(3));
    }

    /// A batch that keeps failing stops being reclaimed once it has used up `max_attempts` picks.
    #[tokio::test]
    async fn reclaim_stops_after_max_attempts() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::latest();
        save_patch(&mut conn, batch_minor, 0).await;
        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        let max_attempts = 3;

        // First pick (attempts -> 1), then two reclaims (attempts -> 2, 3). Each cycle fails the
        // batch back so the reclaim branch can pick it up again.
        for _ in 0..max_attempts {
            let mut dal = conn.airbender_proof_generation_dal();
            let locked = dal
                .lock_batch_for_proving(Duration::ZERO, L1BatchNumber(0), max_attempts, PROVER_VK)
                .await
                .unwrap()
                .expect("batch should be lockable while attempts remain");
            assert_eq!(locked.l1_batch_number, L1BatchNumber(1));
            dal.mark_proof_failed(L1BatchNumber(1), "boom")
                .await
                .unwrap();
        }

        // The batch has now been picked `max_attempts` times — it must no longer be reclaimable.
        let exhausted = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::ZERO, L1BatchNumber(0), max_attempts, PROVER_VK)
            .await
            .unwrap();
        assert!(
            exhausted.is_none(),
            "batch should not be reclaimed after exhausting attempts"
        );
    }

    /// A VK not registered for the batch's minor does not get the batch.
    #[tokio::test]
    async fn lock_skips_batches_without_matching_vk() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::latest();
        save_patch(&mut conn, batch_minor, 0).await;
        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        assert!(
            poll(&pool, NEXT_GEN_VK, NO_RECLAIM).await.is_none(),
            "batch must not be handed to a prover with an unknown VK"
        );

        // The right key still gets the batch.
        assert!(poll(&pool, PROVER_VK, NO_RECLAIM).await.is_some());
    }

    /// The recorded patch is the highest one registered for the *prover's* VK; a newer patch under
    /// a different key is ignored.
    #[tokio::test]
    async fn lock_records_highest_patch_for_the_provers_vk() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::latest();
        save_patch(&mut conn, batch_minor, 0).await;
        save_patch(&mut conn, batch_minor, 3).await;
        // A newer patch rotates to a different Airbender VK.
        save_patch_with_vk(&mut conn, batch_minor, 5, Some(NEXT_GEN_VK)).await;

        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        let locked = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("batch should be lockable");
        assert_eq!(
            locked.protocol_version,
            ProtocolSemanticVersion {
                minor: batch_minor,
                patch: VersionPatch(3),
            }
        );
    }

    /// An unchanged key registered for two minors keeps taking the next sequential batch straight
    /// through a minor bump.
    #[tokio::test]
    async fn one_vk_unlocks_batches_of_all_its_minors() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let old_minor = ProtocolVersionId::Version30;
        let new_minor = ProtocolVersionId::latest();
        save_patch(&mut conn, old_minor, 2).await;
        save_patch(&mut conn, new_minor, 0).await;

        insert_provable_batch(&mut conn, L1BatchNumber(1), old_minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(2), new_minor).await;

        let first = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("old-minor batch should be lockable");
        assert_eq!(first.l1_batch_number, L1BatchNumber(1));
        assert_eq!(first.protocol_version.minor, old_minor);

        let second = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("new-minor batch should be lockable by the same key");
        assert_eq!(second.l1_batch_number, L1BatchNumber(2));
        assert_eq!(second.protocol_version.minor, new_minor);
    }

    /// When the key *and* the minor both change, each generation only gets batches whose minor its
    /// key is registered for — the old key is not blocked, it simply has no patch for the new minor.
    #[tokio::test]
    async fn a_key_only_gets_batches_of_the_minors_registered_for_it() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let old_minor = ProtocolVersionId::Version30;
        let new_minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, old_minor, 2, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, new_minor, 0, Some(NEXT_GEN_VK)).await;

        insert_provable_batch(&mut conn, L1BatchNumber(1), old_minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(2), new_minor).await;

        // The new generation's key is not registered for the old minor, so batch 1 is not for it.
        assert!(
            poll(&pool, NEXT_GEN_VK, NO_RECLAIM).await.is_none(),
            "the new key must not take a batch of a minor it has no patch for"
        );

        let old = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("the old key owns the old-minor batch");
        assert_eq!(old.l1_batch_number, L1BatchNumber(1));
        assert_eq!(old.protocol_version.minor, old_minor);

        // ... and symmetrically, the old key has nothing registered for the new minor.
        assert!(
            poll(&pool, PROVER_VK, NO_RECLAIM).await.is_none(),
            "the old key must not take a batch of a minor it has no patch for"
        );

        let new = poll(&pool, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("the new key owns the new-minor batch");
        assert_eq!(new.l1_batch_number, L1BatchNumber(2));
        assert_eq!(new.protocol_version.minor, new_minor);
        assert_versions_non_decreasing(&mut conn).await;
    }

    /// A timed-out batch is only reclaimed by a VK matching the version recorded at first lock.
    #[tokio::test]
    async fn reclaim_requires_matching_vk() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::latest();
        save_patch(&mut conn, batch_minor, 0).await;
        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        poll(&pool, PROVER_VK, Duration::ZERO)
            .await
            .expect("batch should be lockable");

        assert!(
            poll(&pool, NEXT_GEN_VK, Duration::ZERO).await.is_none(),
            "a prover with a different VK must not reclaim the job"
        );
        assert!(poll(&pool, PROVER_VK, Duration::ZERO).await.is_some());
    }

    /// Two generations polling as separate handler processes must not produce 31.1 -> 31.2 -> 31.1.
    #[tokio::test]
    async fn two_handler_processes_cannot_record_a_decreasing_sequence() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await; // v31.1
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await; // v31.2
        for number in 10..=13 {
            insert_provable_batch(&mut conn, L1BatchNumber(number), minor).await;
        }

        // The old generation takes batches 10 and 11.
        for number in [10, 11] {
            let locked = poll(&pool, PROVER_VK, NO_RECLAIM)
                .await
                .expect("old generation should claim");
            assert_eq!(locked.l1_batch_number, L1BatchNumber(number));
            assert_eq!(locked.protocol_version.patch, VersionPatch(1));
        }

        // A different handler process, on the new generation, takes batch 12.
        let locked = poll(&pool, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("new generation should claim");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(12));
        assert_eq!(locked.protocol_version.patch, VersionPatch(2));

        // Batch 13 must not go out at v31.1; the old generation is starved from here on.
        assert!(
            poll(&pool, PROVER_VK, NO_RECLAIM).await.is_none(),
            "batch 13 must not be claimed at v31.1 after batch 12 went out at v31.2"
        );

        let locked = poll(&pool, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("new generation should claim batch 13");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(13));
        assert_eq!(locked.protocol_version.patch, VersionPatch(2));
        assert_versions_non_decreasing(&mut conn).await;
    }

    /// The version committed for the previous batch is the floor for the next one, whoever asks.
    #[tokio::test]
    async fn a_committed_assignment_is_the_floor_for_the_next_batch() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        insert_provable_batch(&mut conn, L1BatchNumber(10), minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(11), minor).await;

        let first = poll(&pool, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("the new generation takes the first batch");
        assert_eq!(first.protocol_version.patch, VersionPatch(2));

        assert!(
            poll(&pool, PROVER_VK, NO_RECLAIM).await.is_none(),
            "v31.2 is committed, so v31.1 can no longer take the next batch"
        );

        let second = poll(&pool, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("the current generation gets it instead");
        assert_eq!(second.l1_batch_number, L1BatchNumber(11));
        assert_eq!(second.protocol_version.patch, VersionPatch(2));
    }

    /// A handler whose transaction predates another process's v31.2 commit still sees that floor:
    /// at READ COMMITTED each statement snapshots as of the claim, not as of `BEGIN`. This is what
    /// lets handlers restart in any order without a startup seed.
    #[tokio::test]
    async fn a_handler_that_predates_a_newer_commit_still_sees_its_floor() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        for number in 10..=12 {
            insert_provable_batch(&mut conn, L1BatchNumber(number), minor).await;
        }

        let first = poll(&pool, PROVER_VK, NO_RECLAIM).await.unwrap();
        assert_eq!(first.l1_batch_number, L1BatchNumber(10));

        // The stale handler's request begins here, before v31.2 exists anywhere.
        let mut stale_conn = pool.connection().await.unwrap();
        let mut stale_tx = stale_conn.start_transaction().await.unwrap();

        // Meanwhile another process claims batch 11 at v31.2 and commits.
        let newer = poll(&pool, NEXT_GEN_VK, NO_RECLAIM).await.unwrap();
        assert_eq!(newer.l1_batch_number, L1BatchNumber(11));
        assert_eq!(newer.protocol_version.patch, VersionPatch(2));

        // Only now does the stale handler run its claim, inside the transaction it opened earlier.
        let stale = stale_tx
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(NO_RECLAIM, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap();
        assert!(
            stale.is_none(),
            "a handler that started before the v31.2 commit must not assign batch 12 at v31.1"
        );
        stale_tx.commit().await.unwrap();
        assert_versions_non_decreasing(&mut conn).await;
    }

    /// Racing for the same candidate, the loser gets nothing rather than a different batch — the
    /// primary-key collision that covers the window where neither sees the other's uncommitted row.
    #[tokio::test]
    async fn concurrent_generations_produce_at_most_one_assignment() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        // Exactly one claimable batch, so a second assignment could only be a duplicate.
        insert_provable_batch(&mut conn, L1BatchNumber(10), minor).await;
        drop(conn);

        let old = tokio::spawn({
            let pool = pool.clone();
            async move { poll(&pool, PROVER_VK, NO_RECLAIM).await }
        });
        let new = tokio::spawn({
            let pool = pool.clone();
            async move { poll(&pool, NEXT_GEN_VK, NO_RECLAIM).await }
        });
        let (old, new) = (old.await.unwrap(), new.await.unwrap());

        assert_eq!(
            usize::from(old.is_some()) + usize::from(new.is_some()),
            1,
            "exactly one of the two racing generations may be handed batch 10"
        );
        let mut conn = pool.connection().await.unwrap();
        assert_eq!(recorded_versions(&mut conn).await.len(), 1);
    }

    /// A claim that was never committed constrains nothing: after an aborted v31.2 claim the old
    /// generation may still take the batch at v31.1.
    #[tokio::test]
    async fn a_rolled_back_newer_assignment_does_not_constrain_the_next_claim() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        insert_provable_batch(&mut conn, L1BatchNumber(10), minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(11), minor).await;

        let first = poll(&pool, PROVER_VK, NO_RECLAIM).await.unwrap();
        assert_eq!(first.l1_batch_number, L1BatchNumber(10));

        // The new generation claims batch 11 at v31.2, then its request fails (as on a missing
        // object-store input) and the transaction rolls back.
        {
            let mut rolling_back = pool.connection().await.unwrap();
            let mut transaction = rolling_back.start_transaction().await.unwrap();
            let claimed = transaction
                .airbender_proof_generation_dal()
                .lock_batch_for_proving(NO_RECLAIM, L1BatchNumber(0), 10, NEXT_GEN_VK)
                .await
                .unwrap()
                .expect("the new generation should claim batch 11");
            assert_eq!(claimed.protocol_version.patch, VersionPatch(2));
            drop(transaction);
        }

        let retaken = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("batch 11 is free again, and nothing newer was ever committed");
        assert_eq!(retaken.l1_batch_number, L1BatchNumber(11));
        assert_eq!(retaken.protocol_version.patch, VersionPatch(1));
        assert_versions_non_decreasing(&mut conn).await;
        assert_eq!(recorded_versions(&mut conn).await.len(), 2);
    }

    /// Batches do not become claimable in batch order. A prover waits for the gap rather than
    /// jumping it: out-of-order claims are what would let two generations miss each other's
    /// uncommitted claim and commit a decreasing pair.
    #[tokio::test]
    async fn claims_do_not_jump_ahead_of_a_gap() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        // Batch 13's inputs land first; batch 12 is still being processed.
        insert_batch_without_inputs(&mut conn, L1BatchNumber(12), minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(13), minor).await;

        for vk in [PROVER_VK, NEXT_GEN_VK] {
            assert!(
                poll(&pool, vk, NO_RECLAIM).await.is_none(),
                "batch 13 must not be claimed while batch 12 is still unclaimed"
            );
        }

        // Once the gap is filled, work resumes in order.
        mark_inputs_ready(&mut conn, L1BatchNumber(12)).await;
        let locked = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("batch 12 should now be claimable");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(12));
        assert_eq!(locked.protocol_version.patch, VersionPatch(1));

        let locked = poll(&pool, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("batch 13 should follow");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(13));
        assert_eq!(locked.protocol_version.patch, VersionPatch(2));
    }

    /// Patch numbering restarts on a minor bump, so v30.9 -> v31.0 is forward progress: the floor
    /// compares `(minor, patch)` lexicographically, not patches alone.
    #[tokio::test]
    async fn minor_bump_with_restarted_patch_is_not_a_regression() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let old_minor = ProtocolVersionId::Version30;
        let new_minor = ProtocolVersionId::latest();
        assert!(new_minor > old_minor);
        save_patch_with_vk(&mut conn, old_minor, 9, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, new_minor, 0, Some(NEXT_GEN_VK)).await;

        insert_provable_batch(&mut conn, L1BatchNumber(20), old_minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(21), new_minor).await;

        let locked = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("old-minor batch should be claimable");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(20));
        assert_eq!(locked.protocol_version.patch, VersionPatch(9));

        let locked = poll(&pool, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("a minor bump must not read as a version regression");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(21));
        assert_eq!(locked.protocol_version.minor, new_minor);
        assert_eq!(locked.protocol_version.patch, VersionPatch(0));
    }

    /// Starving the old generation of *new* work must not strand what it already holds: it can
    /// still retry its own batches at their recorded version.
    #[tokio::test]
    async fn a_stale_generation_can_still_reclaim_its_own_batch() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        insert_provable_batch(&mut conn, L1BatchNumber(10), minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(11), minor).await;

        let old = poll(&pool, PROVER_VK, NO_RECLAIM).await.unwrap();
        assert_eq!(old.l1_batch_number, L1BatchNumber(10));
        let new = poll(&pool, NEXT_GEN_VK, NO_RECLAIM).await.unwrap();
        assert_eq!(new.l1_batch_number, L1BatchNumber(11));
        assert_eq!(new.protocol_version.patch, VersionPatch(2));

        // Batch 10 times out; the old generation reclaims it at its recorded version.
        let reclaimed = poll(&pool, PROVER_VK, Duration::ZERO)
            .await
            .expect("old generation must still reclaim its own timed-out batch");
        assert_eq!(reclaimed.l1_batch_number, L1BatchNumber(10));
        assert_eq!(reclaimed.protocol_version.patch, VersionPatch(1));
    }

    /// SNARK wrapping is likewise ungated by the floor: a v31.1 FRI proof stays wrappable by the
    /// v31.1 key after v31.2 batches have been assigned.
    #[tokio::test]
    async fn a_stale_generation_can_still_snark_wrap_its_own_batch() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        insert_provable_batch(&mut conn, L1BatchNumber(10), minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(11), minor).await;

        poll(&pool, PROVER_VK, NO_RECLAIM).await.unwrap();
        let new = poll(&pool, NEXT_GEN_VK, NO_RECLAIM).await.unwrap();
        assert_eq!(new.protocol_version.patch, VersionPatch(2));

        conn.airbender_proof_generation_dal()
            .save_proof_artifacts_metadata(L1BatchNumber(10), "fri-blob", "old-prover")
            .await
            .unwrap();
        let snark = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_snark(NO_RECLAIM, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("old generation must still SNARK-wrap its own batch");
        assert_eq!(snark.l1_batch_number, L1BatchNumber(10));
        assert_eq!(snark.protocol_version.patch, VersionPatch(1));
    }

    /// On a pruned node the batch stream starts above `first_processed_batch`; the first claim must
    /// aim at the oldest surviving batch rather than under it forever.
    #[tokio::test]
    async fn the_first_claim_starts_at_the_oldest_surviving_batch() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch(&mut conn, minor, 0).await;
        // Batches 1..=99 are long gone; the table starts at 100.
        insert_provable_batch(&mut conn, L1BatchNumber(100), minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(101), minor).await;

        let first = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("the oldest surviving batch should be claimable");
        assert_eq!(first.l1_batch_number, L1BatchNumber(100));

        let second = poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("and then the one after it");
        assert_eq!(second.l1_batch_number, L1BatchNumber(101));
    }

    /// Lowering `first_processed_batch` under existing assignments must not walk backwards into the
    /// gap beneath them.
    #[tokio::test]
    async fn lowering_the_first_processed_batch_does_not_walk_backwards() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch(&mut conn, minor, 0).await;
        for number in [100, 101, 102] {
            insert_provable_batch(&mut conn, L1BatchNumber(number), minor).await;
        }
        // Assigned while `first_processed_batch` was 100.
        insert_claim_at_version(&mut conn, L1BatchNumber(100), minor, 0).await;
        insert_claim_at_version(&mut conn, L1BatchNumber(101), minor, 0).await;

        // Lowered to 0: the next claim is still 102, not 1.
        let locked = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(NO_RECLAIM, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("the claim should continue past the highest assignment");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(102));
    }

    /// SNARK jobs are gated by the same VK: the wrapper proof must verify against the key
    /// registered for the batch's recorded version.
    #[tokio::test]
    async fn snark_lock_requires_matching_vk() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::latest();
        save_patch(&mut conn, batch_minor, 0).await;
        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        poll(&pool, PROVER_VK, NO_RECLAIM)
            .await
            .expect("batch should be lockable");
        conn.airbender_proof_generation_dal()
            .save_proof_artifacts_metadata(L1BatchNumber(1), "proof_blob", "prover-1")
            .await
            .unwrap();

        let locked = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_snark(NO_RECLAIM, L1BatchNumber(0), 10, NEXT_GEN_VK)
            .await
            .unwrap();
        assert!(
            locked.is_none(),
            "SNARK job must not be handed to a prover with a different VK"
        );

        let locked = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_snark(NO_RECLAIM, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap();
        assert!(locked.is_some());
    }
}
