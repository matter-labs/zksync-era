use std::time::Duration;

use zksync_basic_types::{
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    prover_dal::{BasicWitnessGeneratorJobInfo, StuckJobs, WitnessJobStatus},
    L1BatchNumber,
};
use zksync_db_connection::{
    connection::Connection,
    utils::{duration_to_naive_time, pg_interval_from_duration},
};

use crate::{fri_witness_generator_dal::FriWitnessJobStatus, Prover};

#[derive(Debug)]
pub struct FriBasicWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

impl FriBasicWitnessGeneratorDal<'_, '_> {
    pub async fn save_witness_inputs(
        &mut self,
        block_number: L1BatchNumber,
        witness_inputs_blob_url: &str,
        protocol_version: ProtocolSemanticVersion,
    ) {
        sqlx::query!(
            r#"
            INSERT INTO
            witness_inputs_fri (
                l1_batch_number,
                witness_inputs_blob_url,
                protocol_version,
                status,
                created_at,
                updated_at,
                protocol_version_patch
            )
            VALUES
            ($1, $2, $3, 'queued', NOW(), NOW(), $4)
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            i64::from(block_number.0),
            witness_inputs_blob_url,
            protocol_version.minor as i32,
            protocol_version.patch.0 as i32,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap();
    }

    /// Gets the next job to be executed. Returns the batch number and its corresponding blobs.
    /// The blobs arrive from core via prover gateway, as pubdata, this method loads the blobs.
    pub async fn get_next_basic_circuit_witness_job(
        &mut self,
        protocol_version: ProtocolSemanticVersion,
        picked_by: &str,
    ) -> Option<L1BatchNumber> {
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
            SET
                status = 'in_progress',
                attempts = attempts + 1,
                updated_at = NOW(),
                processing_started_at = NOW(),
                picked_by = $2
            WHERE
                l1_batch_number = (
                    SELECT
                        l1_batch_number
                    FROM
                        witness_inputs_fri
                    WHERE
                        status = 'queued'
                        AND protocol_version = $1
                        AND protocol_version_patch = $3
                    ORDER BY
                        l1_batch_number ASC
                    LIMIT
                        1
                    FOR UPDATE
                    SKIP LOCKED
                )
            RETURNING
            witness_inputs_fri.l1_batch_number
            "#,
            protocol_version.minor as i32,
            picked_by,
            protocol_version.patch.0 as i32,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.l1_batch_number as u32))
    }

    pub async fn set_status_for_basic_witness_job(
        &mut self,
        status: FriWitnessJobStatus,
        block_number: L1BatchNumber,
    ) {
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
            SET
                status = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
                AND status != 'successful'
            "#,
            status.to_string(),
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn mark_witness_job_as_successful(
        &mut self,
        block_number: L1BatchNumber,
        time_taken: Duration,
    ) {
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
            SET
                status = 'successful',
                updated_at = NOW(),
                time_taken = $1
            WHERE
                l1_batch_number = $2
            "#,
            duration_to_naive_time(time_taken),
            i64::from(block_number.0)
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn requeue_stuck_basic_jobs(
        &mut self,
        processing_timeout: Duration,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let processing_timeout = pg_interval_from_duration(processing_timeout);
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
            SET
                status = 'queued',
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                (
                    status = 'in_progress'
                    AND processing_started_at <= NOW() - $1::INTERVAL
                    AND attempts < $2
                )
                OR (
                    status = 'failed'
                    AND attempts < $2
                )
            RETURNING
            l1_batch_number,
            status,
            attempts,
            error,
            picked_by
            "#,
            &processing_timeout,
            max_attempts as i32,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| StuckJobs {
            id: row.l1_batch_number as u64,
            status: row.status,
            attempts: row.attempts as u64,
            circuit_id: None,
            error: row.error,
            picked_by: row.picked_by,
        })
        .collect()
    }

    pub async fn protocol_version_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> ProtocolSemanticVersion {
        let result = sqlx::query!(
            r#"
            SELECT
                protocol_version,
                protocol_version_patch
            FROM
                witness_inputs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        ProtocolSemanticVersion::new(
            ProtocolVersionId::try_from(result.protocol_version.unwrap() as u16).unwrap(),
            VersionPatch(result.protocol_version_patch as u32),
        )
    }

    pub async fn get_basic_witness_generator_job_for_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Option<BasicWitnessGeneratorJobInfo> {
        sqlx::query!(
            r#"
            SELECT
                *
            FROM
                witness_inputs_fri
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| BasicWitnessGeneratorJobInfo {
            l1_batch_number,
            witness_inputs_blob_url: row.witness_inputs_blob_url,
            attempts: row.attempts as u32,
            status: row.status.parse::<WitnessJobStatus>().unwrap(),
            error: row.error,
            created_at: row.created_at,
            updated_at: row.updated_at,
            processing_started_at: row.processing_started_at,
            time_taken: row.time_taken,
            protocol_version: row.protocol_version,
            picked_by: row.picked_by,
        })
    }

    pub async fn requeue_stuck_witness_inputs_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        sqlx::query!(
            r#"
            UPDATE witness_inputs_fri
            SET
                status = 'queued',
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                l1_batch_number = $1
                AND attempts >= $2
                AND (
                    status = 'in_progress'
                    OR status = 'failed'
                )
            RETURNING
            l1_batch_number,
            status,
            attempts,
            error,
            picked_by
            "#,
            i64::from(block_number.0),
            max_attempts as i64
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| StuckJobs {
            id: row.l1_batch_number as u64,
            status: row.status,
            attempts: row.attempts as u64,
            circuit_id: None,
            error: row.error,
            picked_by: row.picked_by,
        })
        .collect()
    }
}
