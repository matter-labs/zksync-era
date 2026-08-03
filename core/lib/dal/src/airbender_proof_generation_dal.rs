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

/// Why a prover received no job, as reported by
/// [`AirbenderProofGenerationDal::diagnose_prover_key`].
#[derive(Debug, Clone, Copy)]
pub struct ProverKeyDiagnosis {
    /// Whether any protocol patch is registered for the prover's key at all. `false` means the
    /// prover is either misdeployed or running ahead of the upgrade that registers its key.
    pub key_is_registered: bool,
    /// Whether a batch has already been claimed at a version newer than any this key is registered
    /// for. Since recorded versions never decrease (see
    /// [`AirbenderProofGenerationDal::lock_batch_for_proving`]), such a prover will never be handed
    /// new work again and can be retired once its in-flight batches are done.
    pub superseded: bool,
}

impl AirbenderProofGenerationDal<'_, '_> {
    /// Locks the lowest unclaimed batch for Airbender FRI proving, if the requesting prover can
    /// prove it. A prover is identified by the Airbender SNARK-wrapper VK hash it carries, and the
    /// version recorded for proving is the batch's own minor version (`l1_batches.protocol_version`)
    /// with the highest patch of that minor registered for that key.
    ///
    /// Recorded versions are kept monotonically non-decreasing in batch number, compared as
    /// `(minor, patch)`: with batches 10, 11 at v31.1 and 12 at v31.2, batch 13 can only go to
    /// v31.2 and a v31.1 prover is told there is no work. `eth_sender` proves batches in order
    /// against the single verification key L1 currently holds, so a version that dips back down
    /// strands that batch permanently.
    ///
    /// Reclaimed batches (Step 1) keep the version recorded when they were first locked, and are
    /// only handed to provers whose key matches it — a batch already at v31.1 has its blob key and
    /// L1 verification key pinned there, so the old generation must be able to retry it even after
    /// newer batches moved on. The corollary is operational: an old prover generation may only be
    /// retired once its in-flight batches have reached `snark_generated`.
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

        // Step 1: Try to reclaim a timed-out or failed batch (row already exists). A batch is only
        // reclaimable while it has retries left (`attempts < max_attempts`); each reclaim bumps
        // `attempts`, so a batch that keeps failing eventually stays in `failed` for good instead
        // of being retried forever. Only batches whose recorded proving version carries the
        // requesting prover's VK are handed out — the recorded version determines the blob key and
        // the key the L1 proof is verified against, so it must match the prover doing the retry.
        // FOR UPDATE SKIP LOCKED ensures parallel provers don't pick the same row.
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
        .fetch_optional(self.storage)
        .await?
        .map(Into::into);

        if locked_batch.is_some() {
            return Ok(locked_batch);
        }

        // Step 2: No reclaimable row — claim the lowest unclaimed batch. Batches whose minor has no
        // patch for this VK are skipped: the inner `JOIN LATERAL` then yields no row.
        //
        // The candidate is deliberately the lowest unclaimed batch rather than the lowest *ready*
        // one, i.e. a prover waits for a gap instead of jumping over it. That is what makes the
        // version checks safe under concurrency: at READ COMMITTED a poller cannot see another's
        // uncommitted claim, but since all pollers pick the same candidate they collide on the
        // primary key and `ON CONFLICT DO NOTHING` leaves the loser with no job. If candidates could
        // diverge, two prover generations could each pass their own version check and still commit a
        // decreasing pair. Nothing is lost: `eth_sender` needs batch N before N+1 anyway.
        //
        // The claimed version must then sit between the closest claimed batch below and the closest
        // above. The upper bound only bites when healing a gap left by an earlier implementation
        // that claimed out of order; in steady state nothing is ever claimed above the candidate.
        let locked_batch = sqlx::query_as!(
            StorageLockedBatch,
            r#"
            INSERT INTO airbender_proof_generation_details (
                l1_batch_number, status, created_at, updated_at, prover_taken_at,
                attempts, protocol_version, protocol_version_patch
            )
            SELECT
                p.l1_batch_number,
                $1,
                NOW(),
                NOW(),
                NOW(),
                1,
                l.protocol_version,
                prover_patch.patch
            FROM proof_generation_details p
            JOIN l1_batches l ON l.number = p.l1_batch_number
            JOIN LATERAL (
                SELECT pp.patch
                FROM protocol_patches pp
                WHERE
                    pp.minor = l.protocol_version
                    AND pp.airbender_snark_wrapper_vk_hash = $3
                ORDER BY pp.patch DESC
                LIMIT 1
            ) prover_patch ON TRUE
            WHERE
                p.vm_run_data_blob_url IS NOT NULL
                AND p.proof_gen_data_blob_url IS NOT NULL
                AND l.protocol_version IS NOT NULL
                AND p.l1_batch_number = (
                    SELECT MIN(l2.number)
                    FROM l1_batches l2
                    WHERE
                        l2.number >= GREATEST($2, 1::BIGINT)
                        AND NOT EXISTS (
                            SELECT 1 FROM airbender_proof_generation_details a
                            WHERE a.l1_batch_number = l2.number
                        )
                )
                AND NOT EXISTS (
                    SELECT 1
                    FROM airbender_proof_generation_details prev
                    WHERE
                        prev.l1_batch_number = (
                            SELECT MAX(a.l1_batch_number)
                            FROM airbender_proof_generation_details a
                            WHERE
                                a.l1_batch_number < p.l1_batch_number
                                AND a.protocol_version IS NOT NULL
                        )
                        AND (prev.protocol_version, prev.protocol_version_patch)
                        > (l.protocol_version, prover_patch.patch)
                )
                AND NOT EXISTS (
                    SELECT 1
                    FROM airbender_proof_generation_details next_claim
                    WHERE
                        next_claim.l1_batch_number = (
                            SELECT MIN(a.l1_batch_number)
                            FROM airbender_proof_generation_details a
                            WHERE
                                a.l1_batch_number > p.l1_batch_number
                                AND a.protocol_version IS NOT NULL
                        )
                        AND (next_claim.protocol_version, next_claim.protocol_version_patch)
                        < (l.protocol_version, prover_patch.patch)
                )
            LIMIT 1
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
        .fetch_optional(self.storage)
        .await?
        .map(Into::into);

        Ok(locked_batch)
    }

    /// Explains why a prover carrying `airbender_vk_hash` was handed no job, so that an operator can
    /// tell a misdeployed or obsolete prover apart from an idle queue — all three look like an
    /// endless stream of empty polls. Answers both questions in one round trip because it is only
    /// consulted after a claim came up empty.
    pub async fn diagnose_prover_key(
        &mut self,
        airbender_vk_hash: H256,
    ) -> DalResult<ProverKeyDiagnosis> {
        let row = sqlx::query!(
            r#"
            WITH
            newest_claim AS (
                SELECT
                    a.protocol_version AS minor,
                    a.protocol_version_patch AS patch
                FROM
                    airbender_proof_generation_details a
                WHERE
                    a.protocol_version IS NOT NULL
                ORDER BY
                    a.l1_batch_number DESC
                LIMIT
                    1
            ),

            prover_best AS (
                SELECT
                    pp.minor,
                    pp.patch
                FROM
                    protocol_patches pp
                WHERE
                    pp.airbender_snark_wrapper_vk_hash = $1
                ORDER BY
                    pp.minor DESC,
                    pp.patch DESC
                LIMIT
                    1
            )

            SELECT
                EXISTS (SELECT 1 FROM prover_best) AS "key_is_registered!",
                EXISTS (
                    SELECT
                        1
                    FROM
                        newest_claim n,
                        prover_best b
                    WHERE
                        (n.minor, n.patch) > (b.minor, b.patch)
                ) AS "superseded!"
            "#,
            airbender_vk_hash.as_bytes()
        )
        .instrument("diagnose_prover_key")
        .with_arg("airbender_vk_hash", &airbender_vk_hash)
        .fetch_one(self.storage)
        .await?;
        Ok(ProverKeyDiagnosis {
            key_is_registered: row.key_is_registered,
            superseded: row.superseded,
        })
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

    /// Inserts a batch whose proving inputs are not on GCS yet, so it is not claimable. Mirrors a
    /// batch BWIP hasn't finished with; it proves several concurrently, so a higher batch can become
    /// claimable before a lower one.
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

    async fn lock_for(
        conn: &mut Connection<'_, Core>,
        vk: H256,
        timeout: Duration,
    ) -> Option<LockedBatch> {
        conn.airbender_proof_generation_dal()
            .lock_batch_for_proving(timeout, L1BatchNumber(0), 10, vk)
            .await
            .unwrap()
    }

    /// Records a claim directly at an explicit version, bypassing the lock. Used to reproduce a gap
    /// left behind by the earlier implementation, which claimed batches out of order.
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

    /// Asserts the core invariant: recorded proving versions never decrease as batch numbers grow.
    async fn assert_versions_non_decreasing(conn: &mut Connection<'_, Core>) {
        let rows = sqlx::query!(
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
        .unwrap();

        let versions: Vec<_> = rows
            .iter()
            .filter_map(|row| {
                row.protocol_version
                    .map(|minor| (row.l1_batch_number, minor, row.protocol_version_patch))
            })
            .collect();
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

    /// The first lock must record the batch's own minor version with the latest patch known for
    /// that minor — *not* the globally latest protocol version.
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

        let locked = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::MAX, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
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

    /// Reclaiming a timed-out batch must preserve the version recorded at first lock instead of
    /// recomputing it.
    #[tokio::test]
    async fn reclaim_preserves_recorded_version() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::Version30;
        save_patch(&mut conn, batch_minor, 0).await;
        save_patch(&mut conn, batch_minor, 3).await;

        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        let first = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::MAX, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("batch should be lockable");

        // A newer patch appears after the batch was first locked.
        save_patch(&mut conn, batch_minor, 7).await;

        // Zero timeout makes the picked batch immediately reclaimable.
        let reclaimed = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::ZERO, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("batch should be reclaimable");

        assert_eq!(reclaimed.l1_batch_number, L1BatchNumber(1));
        assert_eq!(reclaimed.protocol_version, first.protocol_version);
        assert_eq!(reclaimed.protocol_version.patch, VersionPatch(3));
    }

    /// A batch that keeps failing must stop being reclaimed once it has used up `max_attempts`
    /// picks, instead of being retried forever.
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

    /// A prover carrying a VK that is not registered for the batch's minor version must not
    /// receive the batch at all.
    #[tokio::test]
    async fn lock_skips_batches_without_matching_vk() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::latest();
        save_patch(&mut conn, batch_minor, 0).await;
        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        let wrong_key = H256::repeat_byte(0xcd);
        let locked = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::MAX, L1BatchNumber(0), 10, wrong_key)
            .await
            .unwrap();
        assert!(
            locked.is_none(),
            "batch must not be handed to a prover with an unknown VK"
        );

        // The right key still gets the batch.
        let locked = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::MAX, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap();
        assert!(locked.is_some());
    }

    /// The recorded patch must be the highest patch registered for the *prover's* VK — a newer
    /// patch carrying a different (e.g. next prover generation's) VK must be ignored.
    #[tokio::test]
    async fn lock_records_highest_patch_for_the_provers_vk() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::latest();
        let next_gen_vk = H256::repeat_byte(0xcd);
        save_patch(&mut conn, batch_minor, 0).await;
        save_patch(&mut conn, batch_minor, 3).await;
        // A newer patch rotates to a different Airbender VK.
        save_patch_with_vk(&mut conn, batch_minor, 5, Some(next_gen_vk)).await;

        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        let locked = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::MAX, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("batch should be lockable");
        assert_eq!(
            locked.protocol_version,
            ProtocolSemanticVersion {
                minor: batch_minor,
                patch: VersionPatch(3),
            }
        );
    }

    /// One VK registered for two minor versions makes batches of both minors lockable by the same
    /// prover.
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

        // Note: not `Duration::MAX` — `pg_interval_from_duration` wraps it to a bogus (possibly
        // negative) interval, which would make the first lock immediately "reclaimable".
        let timeout = Duration::from_secs(600);
        let first = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(timeout, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("old-minor batch should be lockable");
        assert_eq!(first.l1_batch_number, L1BatchNumber(1));
        assert_eq!(first.protocol_version.minor, old_minor);

        let second = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(timeout, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("new-minor batch should be lockable by the same key");
        assert_eq!(second.l1_batch_number, L1BatchNumber(2));
        assert_eq!(second.protocol_version.minor, new_minor);
    }

    /// A timed-out batch must only be reclaimed by a prover whose VK matches the version recorded
    /// at first lock — the recorded version drives the blob key and the L1 verification key.
    #[tokio::test]
    async fn reclaim_requires_matching_vk() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::latest();
        save_patch(&mut conn, batch_minor, 0).await;
        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        conn.airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::ZERO, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("batch should be lockable");

        let wrong_key = H256::repeat_byte(0xcd);
        let reclaimed = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::ZERO, L1BatchNumber(0), 10, wrong_key)
            .await
            .unwrap();
        assert!(
            reclaimed.is_none(),
            "a prover with a different VK must not reclaim the job"
        );

        let reclaimed = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::ZERO, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap();
        assert!(reclaimed.is_some());
    }

    /// Two prover generations poll at once (v31.1 and v31.2, different keys). Once a batch has gone
    /// out under the newer version, no later batch may go out under the older one: batches 10, 11
    /// at v31.1 and 12 at v31.2 means batch 13 must be v31.2, never v31.1.
    #[tokio::test]
    async fn stale_generation_cannot_claim_after_newer_generation() {
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
            let locked = lock_for(&mut conn, PROVER_VK, NO_RECLAIM)
                .await
                .expect("old generation should claim");
            assert_eq!(locked.l1_batch_number, L1BatchNumber(number));
            assert_eq!(locked.protocol_version.patch, VersionPatch(1));
        }

        // The new generation takes batch 12.
        let locked = lock_for(&mut conn, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("new generation should claim");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(12));
        assert_eq!(locked.protocol_version.patch, VersionPatch(2));

        // Batch 13 must NOT go out at v31.1 — the old generation is starved from here on.
        assert!(
            lock_for(&mut conn, PROVER_VK, NO_RECLAIM).await.is_none(),
            "batch 13 must not be claimed at v31.1 after batch 12 went out at v31.2"
        );

        // The new generation gets it instead.
        let locked = lock_for(&mut conn, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("new generation should claim batch 13");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(13));
        assert_eq!(locked.protocol_version.patch, VersionPatch(2));
    }

    /// Batches do not become claimable in batch order — BWIP proves several concurrently and each
    /// publishes its inputs when it finishes. A prover must wait for the gap rather than jump over
    /// it: claiming out of order is what would let two generations commit a decreasing pair without
    /// either noticing (neither sees the other's uncommitted claim at READ COMMITTED).
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
                lock_for(&mut conn, vk, NO_RECLAIM).await.is_none(),
                "batch 13 must not be claimed while batch 12 is still unclaimed"
            );
        }

        // Once the gap is filled, work resumes in order.
        mark_inputs_ready(&mut conn, L1BatchNumber(12)).await;
        let locked = lock_for(&mut conn, PROVER_VK, NO_RECLAIM)
            .await
            .expect("batch 12 should now be claimable");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(12));
        assert_eq!(locked.protocol_version.patch, VersionPatch(1));

        let locked = lock_for(&mut conn, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("batch 13 should follow");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(13));
        assert_eq!(locked.protocol_version.patch, VersionPatch(2));
    }

    /// A gap left behind by an earlier implementation (which did claim out of order) must be healed
    /// rather than skipped, and healing it must not itself invert the order — so only a generation
    /// whose version fits between the neighbours may take it.
    #[tokio::test]
    async fn a_pre_existing_gap_is_healed_in_order() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        let newest_gen_vk = H256::repeat_byte(0xef);
        save_patch_with_vk(&mut conn, minor, 3, Some(newest_gen_vk)).await;

        for number in 10..=13 {
            insert_provable_batch(&mut conn, L1BatchNumber(number), minor).await;
        }
        // Batches 10, 11 at v31.1 and 13 at v31.2, with 12 left unclaimed — the shape an
        // out-of-order claim would have left behind.
        for number in [10, 11] {
            let locked = lock_for(&mut conn, PROVER_VK, NO_RECLAIM).await.unwrap();
            assert_eq!(locked.l1_batch_number, L1BatchNumber(number));
        }
        insert_claim_at_version(&mut conn, L1BatchNumber(13), minor, 2).await;

        // v31.3 sits above batch 13's v31.2, so it may not fill the gap.
        assert!(
            lock_for(&mut conn, newest_gen_vk, NO_RECLAIM)
                .await
                .is_none(),
            "healing the gap at v31.3 would invert it against batch 13 at v31.2"
        );

        // v31.1 fits between batch 11 (v31.1) and batch 13 (v31.2).
        let locked = lock_for(&mut conn, PROVER_VK, NO_RECLAIM)
            .await
            .expect("the gap must be fillable at a version that keeps the order");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(12));
        assert_eq!(locked.protocol_version.patch, VersionPatch(1));
    }

    /// Two generations polling concurrently must not commit a decreasing pair. The claim runs inside
    /// a transaction that stays open while the prover's inputs are fetched, so a second poller
    /// genuinely races an uncommitted claim; because both compute the same candidate they collide on
    /// the primary key instead of each claiming a different batch.
    #[tokio::test]
    async fn concurrent_generations_cannot_commit_a_decreasing_pair() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        for number in 10..=12 {
            insert_provable_batch(&mut conn, L1BatchNumber(number), minor).await;
        }
        drop(conn);

        // The old generation claims and holds the transaction open, as the request processor does
        // while it downloads the batch's inputs.
        let mut holder = pool.connection().await.unwrap();
        let mut open_tx = holder.start_transaction().await.unwrap();
        let held = open_tx
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(NO_RECLAIM, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("old generation should claim the first batch");
        assert_eq!(held.l1_batch_number, L1BatchNumber(10));

        // The new generation polls while that claim is still uncommitted.
        let racer_pool = pool.clone();
        let racer = tokio::spawn(async move {
            let mut conn = racer_pool.connection().await.unwrap();
            conn.airbender_proof_generation_dal()
                .lock_batch_for_proving(NO_RECLAIM, L1BatchNumber(0), 10, NEXT_GEN_VK)
                .await
                .unwrap()
        });
        // Give the racer time to reach the insert and block on the primary key. If it happens to
        // run after the commit instead, the assertions below still hold — they check the invariant,
        // not the interleaving.
        tokio::time::sleep(Duration::from_millis(300)).await;
        open_tx.commit().await.unwrap();
        let raced = racer.await.unwrap();

        // Whatever the interleaving, the racer must not have taken a *different* batch at a version
        // that inverts the order.
        if let Some(raced) = raced {
            assert_ne!(
                raced.l1_batch_number,
                L1BatchNumber(10),
                "two provers must not both claim the same batch"
            );
            assert_eq!(raced.l1_batch_number, L1BatchNumber(11));
        }
        let mut conn = pool.connection().await.unwrap();
        assert_versions_non_decreasing(&mut conn).await;
    }

    /// Patch numbering restarts on a minor bump, so v30.9 -> v31.0 is forward progress. The guard
    /// compares `(minor, patch)` lexicographically; comparing patches alone would wedge the queue
    /// at the first batch of every new minor version.
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

        let locked = lock_for(&mut conn, PROVER_VK, NO_RECLAIM)
            .await
            .expect("old-minor batch should be claimable");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(20));
        assert_eq!(locked.protocol_version.patch, VersionPatch(9));

        let locked = lock_for(&mut conn, NEXT_GEN_VK, NO_RECLAIM)
            .await
            .expect("a minor bump must not read as a version regression");
        assert_eq!(locked.l1_batch_number, L1BatchNumber(21));
        assert_eq!(locked.protocol_version.minor, new_minor);
        assert_eq!(locked.protocol_version.patch, VersionPatch(0));
    }

    /// Starving the old generation of *new* work must not strand the work it already holds: its
    /// batches keep their recorded version, so it must still be able to retry them and wrap them
    /// into SNARKs — otherwise `eth_sender`, which needs those batches in order, deadlocks.
    #[tokio::test]
    async fn stale_generation_can_still_finish_its_own_batches() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;
        insert_provable_batch(&mut conn, L1BatchNumber(10), minor).await;
        insert_provable_batch(&mut conn, L1BatchNumber(11), minor).await;

        let old = lock_for(&mut conn, PROVER_VK, NO_RECLAIM).await.unwrap();
        assert_eq!(old.l1_batch_number, L1BatchNumber(10));
        let new = lock_for(&mut conn, NEXT_GEN_VK, NO_RECLAIM).await.unwrap();
        assert_eq!(new.l1_batch_number, L1BatchNumber(11));
        assert_eq!(new.protocol_version.patch, VersionPatch(2));

        // Batch 10 times out. Even though a newer version has already gone out for batch 11, the
        // old generation must be able to reclaim its own batch — at its recorded version.
        let reclaimed = lock_for(&mut conn, PROVER_VK, Duration::ZERO)
            .await
            .expect("old generation must still reclaim its own timed-out batch");
        assert_eq!(reclaimed.l1_batch_number, L1BatchNumber(10));
        assert_eq!(reclaimed.protocol_version.patch, VersionPatch(1));

        // And once its FRI proof lands, it must still be wrappable by the old generation.
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

    /// The diagnosis behind an empty poll must distinguish an unregistered key, a superseded
    /// generation, and a simply idle queue.
    #[tokio::test]
    async fn diagnose_prover_key_reports_why_there_is_no_work() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let minor = ProtocolVersionId::latest();
        save_patch_with_vk(&mut conn, minor, 1, Some(PROVER_VK)).await;
        save_patch_with_vk(&mut conn, minor, 2, Some(NEXT_GEN_VK)).await;

        // A key nothing is registered for.
        let unknown = conn
            .airbender_proof_generation_dal()
            .diagnose_prover_key(H256::repeat_byte(0x99))
            .await
            .unwrap();
        assert!(!unknown.key_is_registered);
        assert!(!unknown.superseded);

        // Registered keys, with nothing claimed yet: the queue is simply idle.
        for vk in [PROVER_VK, NEXT_GEN_VK] {
            let idle = conn
                .airbender_proof_generation_dal()
                .diagnose_prover_key(vk)
                .await
                .unwrap();
            assert!(idle.key_is_registered);
            assert!(!idle.superseded, "nothing is claimed yet");
        }

        // Once a batch is claimed at v31.2, the v31.1 generation is superseded but the v31.2 one
        // is not.
        insert_provable_batch(&mut conn, L1BatchNumber(10), minor).await;
        lock_for(&mut conn, NEXT_GEN_VK, NO_RECLAIM).await.unwrap();

        let old_gen = conn
            .airbender_proof_generation_dal()
            .diagnose_prover_key(PROVER_VK)
            .await
            .unwrap();
        assert!(old_gen.key_is_registered);
        assert!(old_gen.superseded);

        let new_gen = conn
            .airbender_proof_generation_dal()
            .diagnose_prover_key(NEXT_GEN_VK)
            .await
            .unwrap();
        assert!(!new_gen.superseded);
    }

    /// SNARK wrapping jobs are gated by the same VK: the wrapper proof must verify against the
    /// key registered for the batch's recorded protocol version.
    #[tokio::test]
    async fn snark_lock_requires_matching_vk() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        let batch_minor = ProtocolVersionId::latest();
        save_patch(&mut conn, batch_minor, 0).await;
        insert_provable_batch(&mut conn, L1BatchNumber(1), batch_minor).await;

        conn.airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::MAX, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap()
            .expect("batch should be lockable");
        conn.airbender_proof_generation_dal()
            .save_proof_artifacts_metadata(L1BatchNumber(1), "proof_blob", "prover-1")
            .await
            .unwrap();

        let wrong_key = H256::repeat_byte(0xcd);
        let locked = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_snark(Duration::MAX, L1BatchNumber(0), 10, wrong_key)
            .await
            .unwrap();
        assert!(
            locked.is_none(),
            "SNARK job must not be handed to a prover with a different VK"
        );

        let locked = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_snark(Duration::MAX, L1BatchNumber(0), 10, PROVER_VK)
            .await
            .unwrap();
        assert!(locked.is_some());
    }
}
