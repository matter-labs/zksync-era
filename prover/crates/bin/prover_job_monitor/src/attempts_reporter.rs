use async_trait::async_trait;
use zksync_config::configs::{
    FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig,
};
use zksync_db_connection::connection::Connection;
use zksync_prover_dal::{Prover, ProverDal};
use zksync_types::basic_fri_types::AggregationRound;

use crate::{
    metrics::{JobType, PROVER_JOB_MONITOR_METRICS},
    task_wiring::Task,
};

pub struct ProverJobAttemptsReporter {
    pub prover_config: FriProverConfig,
    pub witness_generator_config: FriWitnessGeneratorConfig,
    pub compressor_config: FriProofCompressorConfig,
}

impl ProverJobAttemptsReporter {
    pub async fn check_witness_generator_job_attempts(
        &self,
        connection: &mut Connection<'_, Prover>,
        round: AggregationRound,
    ) -> anyhow::Result<()> {
        let max_attempts = self.witness_generator_config.max_attempts;
        match round {
            AggregationRound::BasicCircuits => {
                let jobs = connection
                    .fri_basic_witness_generator_dal()
                    .check_reached_max_attempts(max_attempts)
                    .await;
                PROVER_JOB_MONITOR_METRICS.reached_max_attempts[&JobType::BasicWitnessGenerator]
                    .set(jobs.len() as i64);
                if jobs.len() > 0 {
                    tracing::warn!(
                        "Basic witness generator jobs reached max attempts: {:?}",
                        jobs
                    );
                }
            }
            AggregationRound::LeafAggregation => {
                let jobs = connection
                    .fri_leaf_witness_generator_dal()
                    .check_reached_max_attempts(max_attempts)
                    .await;
                PROVER_JOB_MONITOR_METRICS.reached_max_attempts[&JobType::LeafWitnessGenerator]
                    .set(jobs.len() as i64);
                if jobs.len() > 0 {
                    tracing::warn!(
                        "Leaf witness generator jobs reached max attempts: {:?}",
                        jobs
                    );
                }
            }
            AggregationRound::NodeAggregation => {
                let jobs = connection
                    .fri_node_witness_generator_dal()
                    .check_reached_max_attempts(max_attempts)
                    .await;
                PROVER_JOB_MONITOR_METRICS.reached_max_attempts[&JobType::NodeWitnessGenerator]
                    .set(jobs.len() as i64);
                if jobs.len() > 0 {
                    tracing::warn!(
                        "Node witness generator jobs reached max attempts: {:?}",
                        jobs
                    );
                }
            }
            AggregationRound::RecursionTip => {
                let jobs = connection
                    .fri_recursion_tip_witness_generator_dal()
                    .check_reached_max_attempts(max_attempts)
                    .await;
                PROVER_JOB_MONITOR_METRICS.reached_max_attempts
                    [&JobType::RecursionTipWitnessGenerator]
                    .set(jobs.len() as i64);
                if jobs.len() > 0 {
                    tracing::warn!(
                        "Recursion Tip witness generator jobs reached max attempts: {:?}",
                        jobs
                    );
                }
            }
            AggregationRound::Scheduler => {
                let jobs = connection
                    .fri_scheduler_witness_generator_dal()
                    .check_reached_max_attempts(max_attempts)
                    .await;
                PROVER_JOB_MONITOR_METRICS.reached_max_attempts
                    [&JobType::SchedulerWitnessGenerator]
                    .set(jobs.len() as i64);
                if jobs.len() > 0 {
                    tracing::warn!(
                        "Scheduler witness generator jobs reached max attempts: {:?}",
                        jobs
                    );
                }
            }
        }

        Ok(())
    }

    pub async fn check_prover_job_attempts(
        &self,
        connection: &mut Connection<'_, Prover>,
    ) -> anyhow::Result<()> {
        let max_attempts = self.prover_config.max_attempts;
        let jobs = connection
            .fri_prover_jobs_dal()
            .check_reached_max_attempts(max_attempts)
            .await;
        PROVER_JOB_MONITOR_METRICS.reached_max_attempts[&JobType::ProverFri].set(jobs.len() as i64);
        if jobs.len() > 0 {
            tracing::warn!("Prover jobs reached max attempts: {:?}", jobs);
        }

        Ok(())
    }

    pub async fn check_proof_compressor_job_attempts(
        &self,
        connection: &mut Connection<'_, Prover>,
    ) -> anyhow::Result<()> {
        let max_attempts = self.compressor_config.max_attempts;
        let jobs = connection
            .fri_proof_compressor_dal()
            .check_reached_max_attempts(max_attempts)
            .await;
        PROVER_JOB_MONITOR_METRICS.reached_max_attempts[&JobType::ProofCompressor]
            .set(jobs.len() as i64);
        if jobs.len() > 0 {
            tracing::warn!("Proof compressor jobs reached max attempts: {:?}", jobs);
        }

        Ok(())
    }
}

#[async_trait]
impl Task for ProverJobAttemptsReporter {
    async fn invoke(&self, connection: &mut Connection<Prover>) -> anyhow::Result<()> {
        self.check_witness_generator_job_attempts(connection, AggregationRound::BasicCircuits)
            .await?;
        self.check_witness_generator_job_attempts(connection, AggregationRound::LeafAggregation)
            .await?;
        self.check_witness_generator_job_attempts(connection, AggregationRound::NodeAggregation)
            .await?;
        self.check_witness_generator_job_attempts(connection, AggregationRound::RecursionTip)
            .await?;
        self.check_witness_generator_job_attempts(connection, AggregationRound::Scheduler)
            .await?;

        self.check_prover_job_attempts(connection).await?;

        self.check_proof_compressor_job_attempts(connection).await?;

        Ok(())
    }
}
