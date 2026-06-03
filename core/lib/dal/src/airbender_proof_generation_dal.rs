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
use zksync_types::{protocol_version::ProtocolSemanticVersion, L1BatchNumber};

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
    /// Locks the oldest provable batch for Airbender FRI proving.
    ///
    /// On the first lock of a batch (Step 2), the protocol version recorded for proving is the
    /// batch's own minor version (`l1_batches.protocol_version`) combined with the latest patch
    /// known for that minor in `protocol_patches`. This is deliberately *not* the globally latest
    /// version: a batch must be proven under the protocol it was executed with, only picking up
    /// the newest patch (e.g. an updated verification key) for that minor. Reclaimed batches
    /// (Step 1) keep the version recorded when they were first locked.
    pub async fn lock_batch_for_proving(
        &mut self,
        processing_timeout: Duration,
        min_batch_number: L1BatchNumber,
    ) -> DalResult<Option<LockedBatch>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let min_batch_number = i64::from(min_batch_number.0);
        let picked = AirbenderProofGenerationJobStatus::PickedByProver.to_string();
        let failed = AirbenderProofGenerationJobStatus::Failed.to_string();

        // Step 1: Try to reclaim a timed-out or failed batch (row already exists).
        // FOR UPDATE SKIP LOCKED ensures parallel provers don't pick the same row.
        let locked_batch = sqlx::query_as!(
            StorageLockedBatch,
            r#"
            UPDATE airbender_proof_generation_details
            SET
                status = $1, updated_at = NOW(), prover_taken_at = NOW()
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
                        AND (
                            apgd.status = $2
                            OR (
                                apgd.status = $1
                                AND apgd.prover_taken_at < NOW() - $4::INTERVAL
                            )
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
        )
        .instrument("lock_batch_for_proving#reclaim")
        .with_arg("processing_timeout", &processing_timeout)
        .with_arg("min_batch_number", &min_batch_number)
        .fetch_optional(self.storage)
        .await?
        .map(Into::into);

        if locked_batch.is_some() {
            return Ok(locked_batch);
        }

        // Step 2: No reclaimable row — try to claim a new batch.
        // The recorded version is the batch's own minor version with the latest patch known for
        // that minor, so the batch is proven under the protocol it executed with (newest patch
        // only). Batches whose minor has no patch in `protocol_patches` are skipped — the proving
        // version is unknown, and `protocol_version_patch` is NOT NULL so an empty patch can't be
        // inserted anyway.
        // ON CONFLICT DO NOTHING: if two provers race, one wins and the other gets nothing.
        let locked_batch = sqlx::query_as!(
            StorageLockedBatch,
            r#"
            INSERT INTO airbender_proof_generation_details (
                l1_batch_number, status, created_at, updated_at, prover_taken_at,
                protocol_version, protocol_version_patch
            )
            SELECT
                p.l1_batch_number,
                $1,
                NOW(),
                NOW(),
                NOW(),
                l.protocol_version,
                (
                    SELECT pp.patch
                    FROM protocol_patches pp
                    WHERE pp.minor = l.protocol_version
                    ORDER BY pp.patch DESC
                    LIMIT 1
                )
            FROM proof_generation_details p
            JOIN l1_batches l ON l.number = p.l1_batch_number
            WHERE
                p.l1_batch_number >= $2
                AND p.vm_run_data_blob_url IS NOT NULL
                AND p.proof_gen_data_blob_url IS NOT NULL
                AND l.protocol_version IS NOT NULL
                AND EXISTS (
                    SELECT 1 FROM protocol_patches pp
                    WHERE pp.minor = l.protocol_version
                )
                AND NOT EXISTS (
                    SELECT 1 FROM airbender_proof_generation_details a
                    WHERE a.l1_batch_number = p.l1_batch_number
                )
            ORDER BY p.l1_batch_number ASC
            LIMIT 1
            ON CONFLICT (l1_batch_number) DO NOTHING
            RETURNING l1_batch_number,
            created_at,
            protocol_version AS "protocol_version!",
            protocol_version_patch
            "#,
            picked,
            min_batch_number,
        )
        .instrument("lock_batch_for_proving#new")
        .with_arg("min_batch_number", &min_batch_number)
        .fetch_optional(self.storage)
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

    /// Lock a batch for SNARK wrapping. Picks the oldest batch whose FRI proof has been
    /// submitted (`status = 'generated'`), or reclaims a `picked_for_snark` batch whose
    /// `snark_taken_at` exceeded `processing_timeout`.
    pub async fn lock_batch_for_snark(
        &mut self,
        processing_timeout: Duration,
        min_batch_number: L1BatchNumber,
    ) -> DalResult<Option<LockedBatch>> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        let min_batch_number = i64::from(min_batch_number.0);
        let picked_for_snark = AirbenderProofGenerationJobStatus::PickedForSnark.to_string();
        let generated = AirbenderProofGenerationJobStatus::Generated.to_string();

        let locked_batch = sqlx::query_as!(
            StorageLockedBatch,
            r#"
            UPDATE airbender_proof_generation_details
            SET status = $1, updated_at = NOW(), snark_taken_at = NOW()
            WHERE
                l1_batch_number = (
                    SELECT apgd.l1_batch_number
                    FROM airbender_proof_generation_details apgd
                    WHERE
                        apgd.l1_batch_number >= $3
                        AND apgd.proof_blob_url IS NOT NULL
                        AND (
                            apgd.status = $2
                            OR (
                                apgd.status = $1
                                AND apgd.snark_taken_at < NOW() - $4::INTERVAL
                            )
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
        )
        .instrument("lock_batch_for_snark")
        .with_arg("processing_timeout", &processing_timeout)
        .with_arg("min_batch_number", &min_batch_number)
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

    /// Number of batches whose FRI proof has been submitted (`status = 'generated'`) and are
    /// waiting to be wrapped into a SNARK proof.
    pub async fn get_ready_for_snark_count(
        &mut self,
        min_batch_number: L1BatchNumber,
    ) -> DalResult<i64> {
        let min_batch_number = i64::from(min_batch_number.0);
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
            "#,
            min_batch_number,
            AirbenderProofGenerationJobStatus::Generated.to_string(),
        )
        .instrument("get_ready_for_snark_count")
        .with_arg("min_batch_number", &min_batch_number)
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

    async fn save_patch(conn: &mut Connection<'_, Core>, minor: ProtocolVersionId, patch: u32) {
        conn.protocol_versions_dal()
            .save_protocol_version(
                ProtocolSemanticVersion {
                    minor,
                    patch: VersionPatch(patch),
                },
                0,
                L1VerifierConfig::default(),
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
        conn.proof_generation_dal()
            .save_vm_runner_artifacts_metadata(number, "vm_run")
            .await
            .unwrap();
        conn.proof_generation_dal()
            .save_merkle_paths_artifacts_metadata(number, "merkle_paths")
            .await
            .unwrap();
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
            .lock_batch_for_proving(Duration::MAX, L1BatchNumber(0))
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
            .lock_batch_for_proving(Duration::MAX, L1BatchNumber(0))
            .await
            .unwrap()
            .expect("batch should be lockable");

        // A newer patch appears after the batch was first locked.
        save_patch(&mut conn, batch_minor, 7).await;

        // Zero timeout makes the picked batch immediately reclaimable.
        let reclaimed = conn
            .airbender_proof_generation_dal()
            .lock_batch_for_proving(Duration::ZERO, L1BatchNumber(0))
            .await
            .unwrap()
            .expect("batch should be reclaimable");

        assert_eq!(reclaimed.l1_batch_number, L1BatchNumber(1));
        assert_eq!(reclaimed.protocol_version, first.protocol_version);
        assert_eq!(reclaimed.protocol_version.patch, VersionPatch(3));
    }
}
