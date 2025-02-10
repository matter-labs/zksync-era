#![doc = include_str!("../../doc/FriWitnessGeneratorDal.md")]

pub mod basic;
pub mod leaf;
pub mod node;
pub mod recursion_tip;
pub mod scheduler;

use std::collections::HashMap;

use sqlx::{types::chrono::NaiveDateTime, Row};
use zksync_basic_types::{
    basic_fri_types::AggregationRound,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId, VersionPatch},
    prover_dal::{JobCountStatistics, ProofGenerationTime, StuckJobs},
    L1BatchNumber,
};
use zksync_db_connection::{connection::Connection, utils::naive_time_from_pg_interval};

use crate::Prover;

#[derive(Debug)]
pub struct FriWitnessGeneratorDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Prover>,
}

#[derive(Debug, strum::Display, strum::EnumString, strum::AsRefStr)]
pub enum FriWitnessJobStatus {
    #[strum(serialize = "failed")]
    Failed,
    #[strum(serialize = "skipped")]
    Skipped,
    #[strum(serialize = "successful")]
    Successful,
    #[strum(serialize = "in_progress")]
    InProgress,
    #[strum(serialize = "queued")]
    Queued,
}

impl FriWitnessGeneratorDal<'_, '_> {
    pub async fn get_witness_job_attempts(
        &mut self,
        job_id: u32,
        aggregation_round: AggregationRound,
    ) -> sqlx::Result<Option<u32>> {
        let table = match aggregation_round {
            AggregationRound::BasicCircuits => "witness_inputs_fri",
            AggregationRound::LeafAggregation => "leaf_aggregation_witness_jobs_fri",
            AggregationRound::NodeAggregation => "node_aggregation_witness_jobs_fri",
            AggregationRound::RecursionTip => "recursion_tip_witness_jobs_fri",
            AggregationRound::Scheduler => "scheduler_witness_jobs_fri",
        };

        let job_id_column = match aggregation_round {
            AggregationRound::BasicCircuits => "l1_batch_number",
            AggregationRound::LeafAggregation => "id",
            AggregationRound::NodeAggregation => "id",
            AggregationRound::RecursionTip => "l1_batch_number",
            AggregationRound::Scheduler => "l1_batch_number ",
        };

        let query = format!(
            r#"
            SELECT
                attempts
            FROM
                {table}
            WHERE
                {job_id_column} = {job_id}
            "#,
        );

        let attempts = sqlx::query(&query)
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| row.get::<i16, &str>("attempts") as u32);

        Ok(attempts)
    }

    pub async fn mark_witness_job_failed(
        &mut self,
        error: &str,
        job_id: u32,
        aggregation_round: AggregationRound,
    ) {
        let table = match aggregation_round {
            AggregationRound::BasicCircuits => "witness_inputs_fri",
            AggregationRound::LeafAggregation => "leaf_aggregation_witness_jobs_fri",
            AggregationRound::NodeAggregation => "node_aggregation_witness_jobs_fri",
            AggregationRound::RecursionTip => "recursion_tip_witness_jobs_fri",
            AggregationRound::Scheduler => "scheduler_witness_jobs_fri",
        };

        let job_id_column = match aggregation_round {
            AggregationRound::BasicCircuits => "l1_batch_number",
            AggregationRound::LeafAggregation => "id",
            AggregationRound::NodeAggregation => "id",
            AggregationRound::RecursionTip => "l1_batch_number",
            AggregationRound::Scheduler => "l1_batch_number ",
        };

        let query = format!(
            r#"
            UPDATE {table}
            SET
                status = 'failed',
                error = {error},
                updated_at = NOW()
            WHERE
                {job_id_column} = {job_id}
                AND status != 'successful'
            "#,
        );

        sqlx::query(&query)
            .execute(self.storage.conn())
            .await
            .unwrap();
    }

    pub async fn get_witness_jobs_stats(
        &mut self,
        aggregation_round: AggregationRound,
    ) -> HashMap<ProtocolSemanticVersion, JobCountStatistics> {
        let table_name = Self::input_table_name_for(aggregation_round);
        let sql = format!(
            r#"
                SELECT
                    protocol_version,
                    protocol_version_patch,
                    COUNT(*) FILTER (WHERE status = 'queued') as queued,
                    COUNT(*) FILTER (WHERE status = 'in_progress') as in_progress
                FROM
                    {}
                WHERE protocol_version IS NOT NULL
                GROUP BY
                    protocol_version,
                    protocol_version_patch
                "#,
            table_name,
        );
        sqlx::query(&sql)
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| {
                let protocol_semantic_version = ProtocolSemanticVersion::new(
                    ProtocolVersionId::try_from(row.get::<i32, &str>("protocol_version") as u16)
                        .unwrap(),
                    VersionPatch(row.get::<i32, &str>("protocol_version_patch") as u32),
                );
                let key = protocol_semantic_version;
                let value = JobCountStatistics {
                    queued: row.get::<i64, &str>("queued") as usize,
                    in_progress: row.get::<i64, &str>("in_progress") as usize,
                };
                (key, value)
            })
            .collect()
    }

    fn input_table_name_for(aggregation_round: AggregationRound) -> &'static str {
        match aggregation_round {
            AggregationRound::BasicCircuits => "witness_inputs_fri",
            AggregationRound::LeafAggregation => "leaf_aggregation_witness_jobs_fri",
            AggregationRound::NodeAggregation => "node_aggregation_witness_jobs_fri",
            AggregationRound::RecursionTip => "recursion_tip_witness_jobs_fri",
            AggregationRound::Scheduler => "scheduler_witness_jobs_fri",
        }
    }

    pub async fn delete_witness_generator_data_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        aggregation_round: AggregationRound,
    ) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        sqlx::query(
            format!(
                r#"
            DELETE FROM
                {table}
            WHERE
                l1_batch_number = {l1_batch_number}
            "#,
                table = Self::input_table_name_for(aggregation_round),
                l1_batch_number = i64::from(block_number.0),
            )
            .as_str(),
        )
        .execute(self.storage.conn())
        .await
    }

    pub async fn delete_batch_data(
        &mut self,
        block_number: L1BatchNumber,
    ) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        self.delete_witness_generator_data_for_batch(block_number, AggregationRound::BasicCircuits)
            .await?;
        self.delete_witness_generator_data_for_batch(
            block_number,
            AggregationRound::LeafAggregation,
        )
        .await?;
        self.delete_witness_generator_data_for_batch(
            block_number,
            AggregationRound::NodeAggregation,
        )
        .await?;
        self.delete_witness_generator_data(AggregationRound::RecursionTip)
            .await?;
        self.delete_witness_generator_data_for_batch(block_number, AggregationRound::Scheduler)
            .await
    }

    pub async fn delete_witness_generator_data(
        &mut self,
        aggregation_round: AggregationRound,
    ) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        sqlx::query(
            format!(
                r#"
            DELETE FROM
                {}
            "#,
                Self::input_table_name_for(aggregation_round)
            )
            .as_str(),
        )
        .execute(self.storage.conn())
        .await
    }

    pub async fn delete(&mut self) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
        self.delete_witness_generator_data(AggregationRound::BasicCircuits)
            .await?;
        self.delete_witness_generator_data(AggregationRound::LeafAggregation)
            .await?;
        self.delete_witness_generator_data(AggregationRound::NodeAggregation)
            .await?;
        self.delete_witness_generator_data(AggregationRound::RecursionTip)
            .await?;
        self.delete_witness_generator_data(AggregationRound::Scheduler)
            .await
    }

    pub async fn requeue_stuck_leaf_aggregation_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        self.requeue_stuck_jobs_for_batch_in_aggregation_round(
            AggregationRound::LeafAggregation,
            block_number,
            max_attempts,
        )
        .await
    }

    pub async fn requeue_stuck_node_aggregation_jobs_for_batch(
        &mut self,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        self.requeue_stuck_jobs_for_batch_in_aggregation_round(
            AggregationRound::NodeAggregation,
            block_number,
            max_attempts,
        )
        .await
    }

    async fn requeue_stuck_jobs_for_batch_in_aggregation_round(
        &mut self,
        aggregation_round: AggregationRound,
        block_number: L1BatchNumber,
        max_attempts: u32,
    ) -> Vec<StuckJobs> {
        let table_name = Self::input_table_name_for(aggregation_round);
        let job_id_table_name = Self::job_id_table_name_for(aggregation_round);
        let query = format!(
            r#"
            UPDATE {}
            SET
                status = 'queued',
                updated_at = NOW(),
                processing_started_at = NOW()
            WHERE
                l1_batch_number = {}
                AND attempts >= {}
                AND (status = 'in_progress' OR status = 'failed')
            RETURNING
                {},
                status,
                attempts,
                circuit_id,
                error,
                picked_by
            "#,
            table_name,
            i64::from(block_number.0),
            max_attempts,
            job_id_table_name
        );
        sqlx::query(&query)
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| StuckJobs {
                id: row.get::<i64, &str>(job_id_table_name) as u64,
                status: row.get("status"),
                attempts: row.get::<i16, &str>("attempts") as u64,
                circuit_id: Some(row.get::<i16, &str>("circuit_id") as u32),
                error: row.get("error"),
                picked_by: row.get("picked_by"),
            })
            .collect()
    }

    fn job_id_table_name_for(aggregation_round: AggregationRound) -> &'static str {
        match aggregation_round {
            AggregationRound::BasicCircuits
            | AggregationRound::RecursionTip
            | AggregationRound::Scheduler => "l1_batch_number",
            AggregationRound::LeafAggregation | AggregationRound::NodeAggregation => "id",
        }
    }

    pub async fn get_proof_generation_times_for_time_frame(
        &mut self,
        time_frame: NaiveDateTime,
    ) -> sqlx::Result<Vec<ProofGenerationTime>> {
        let proof_generation_times = sqlx::query!(
            r#"
            SELECT
                comp.l1_batch_number,
                (comp.updated_at - wit.created_at) AS time_taken,
                wit.created_at
            FROM
                proof_compression_jobs_fri AS comp
            JOIN witness_inputs_fri AS wit ON comp.l1_batch_number = wit.l1_batch_number
            WHERE
                wit.created_at > $1
            ORDER BY
                time_taken DESC;
            "#,
            time_frame.into(),
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|row| ProofGenerationTime {
            l1_batch_number: L1BatchNumber(row.l1_batch_number as u32),
            time_taken: naive_time_from_pg_interval(
                row.time_taken.expect("time_taken must be present"),
            ),
            created_at: row.created_at,
        })
        .collect();
        Ok(proof_generation_times)
    }
}
