use std::collections::HashMap;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use zksync_db_connection::error::DalError;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_types::{
    basic_fri_types::AggregationRound, protocol_version::ProtocolSemanticVersion,
    prover_dal::JobCountStatistics,
};

#[derive(Debug, Clone)]
pub struct AutoscalerQueueReporter {
    connection_pool: ConnectionPool<Prover>,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct QueueReport {
    pub basic_witness_jobs: JobCountStatistics,
    pub leaf_witness_jobs: JobCountStatistics,
    pub node_witness_jobs: JobCountStatistics,
    pub recursion_tip_witness_jobs: JobCountStatistics,
    pub scheduler_witness_jobs: JobCountStatistics,
    pub prover_jobs: JobCountStatistics,
    pub proof_compressor_jobs: JobCountStatistics,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct VersionedQueueReport {
    pub version: ProtocolSemanticVersion,
    pub report: QueueReport,
}

impl AutoscalerQueueReporter {
    pub fn new(connection_pool: ConnectionPool<Prover>) -> Self {
        Self { connection_pool }
    }

    pub async fn get_report(&self) -> Result<Json<Vec<VersionedQueueReport>>, ProcessorError> {
        tracing::debug!("Received request to get queue report");

        let mut result = HashMap::<ProtocolSemanticVersion, QueueReport>::new();

        for round in AggregationRound::ALL_ROUNDS {
            self.get_witness_jobs_report(round, &mut result).await?;
        }

        self.get_prover_jobs_report(&mut result).await?;
        self.get_proof_compressor_jobs_report(&mut result).await?;

        Ok(Json(
            result
                .into_iter()
                .map(|(version, report)| VersionedQueueReport { version, report })
                .collect(),
        ))
    }

    async fn get_witness_jobs_report(
        &self,
        aggregation_round: AggregationRound,
        state: &mut HashMap<ProtocolSemanticVersion, QueueReport>,
    ) -> anyhow::Result<()> {
        let stats = self
            .connection_pool
            .connection()
            .await?
            .fri_witness_generator_dal()
            .get_witness_jobs_stats(aggregation_round)
            .await;

        for (protocol_version, job_stats) in stats {
            let report = state.entry(protocol_version).or_default();

            match aggregation_round {
                AggregationRound::BasicCircuits => report.basic_witness_jobs = job_stats,
                AggregationRound::LeafAggregation => report.leaf_witness_jobs = job_stats,
                AggregationRound::NodeAggregation => report.node_witness_jobs = job_stats,
                AggregationRound::RecursionTip => report.recursion_tip_witness_jobs = job_stats,
                AggregationRound::Scheduler => report.scheduler_witness_jobs = job_stats,
            }
        }
        Ok(())
    }

    async fn get_prover_jobs_report(
        &self,
        state: &mut HashMap<ProtocolSemanticVersion, QueueReport>,
    ) -> anyhow::Result<()> {
        let stats = self
            .connection_pool
            .connection()
            .await?
            .fri_prover_jobs_dal()
            .get_generic_prover_jobs_stats()
            .await;

        for (protocol_version, stats) in stats {
            let report = state.entry(protocol_version).or_default();

            report.prover_jobs = stats;
        }
        Ok(())
    }

    async fn get_proof_compressor_jobs_report(
        &self,
        state: &mut HashMap<ProtocolSemanticVersion, QueueReport>,
    ) -> anyhow::Result<()> {
        let stats = self
            .connection_pool
            .connection()
            .await?
            .fri_proof_compressor_dal()
            .get_jobs_stats()
            .await;

        for (protocol_version, stats) in stats {
            let report = state.entry(protocol_version).or_default();

            report.proof_compressor_jobs = stats;
        }

        Ok(())
    }
}

pub fn get_queue_reporter_router(connection_pool: ConnectionPool<Prover>) -> Router {
    let autoscaler_queue_reporter = AutoscalerQueueReporter::new(connection_pool);

    Router::new().route(
        "/queue_report",
        get(move || async move { autoscaler_queue_reporter.get_report().await }),
    )
}

pub enum ProcessorError {
    Dal(DalError),
    Custom(String),
}

impl From<DalError> for ProcessorError {
    fn from(err: DalError) -> Self {
        ProcessorError::Dal(err)
    }
}

impl From<anyhow::Error> for ProcessorError {
    fn from(err: anyhow::Error) -> Self {
        ProcessorError::Custom(err.to_string())
    }
}

impl IntoResponse for ProcessorError {
    fn into_response(self) -> Response {
        let (status_code, message) = match self {
            ProcessorError::Dal(err) => {
                tracing::error!("Sqlx error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed getting data from database",
                )
            }
            ProcessorError::Custom(err) => {
                tracing::error!("Custom error invoked: {:?}", &err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
            }
        };
        (status_code, message).into_response()
    }
}
