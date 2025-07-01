use anyhow::Context;
use zksync_db_connection::connection::Connection;
use zksync_prover_config::{FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig};
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_task::Task;
use zksync_types::basic_fri_types::AggregationRound;

use crate::metrics::{JobType, PROVER_JOB_MONITOR_METRICS};

pub struct ProverJobAttemptsReporter {
    pool: ConnectionPool<Prover>,
    prover_config: FriProverConfig,
    witness_generator_config: FriWitnessGeneratorConfig,
    compressor_config: FriProofCompressorConfig,
}

impl ProverJobAttemptsReporter {
    pub fn new(
        pool: ConnectionPool<Prover>,
        prover_config: FriProverConfig,
        witness_generator_config: FriWitnessGeneratorConfig,
        compressor_config: FriProofCompressorConfig,
    ) -> Self {
        Self {
            pool,
            prover_config,
            witness_generator_config,
            compressor_config,
        }
    }

    pub async fn check_witness_generator_job_attempts(
        &self,
        connection: &mut Connection<'_, Prover>,
        round: AggregationRound,
    ) -> anyhow::Result<()> {
        let max_attempts = self.witness_generator_config.max_attempts;
        match round {
            AggregationRound::BasicCircuits => {
                PROVER_JOB_MONITOR_METRICS.report_reached_max_attempts(
                    JobType::BasicWitnessGenerator,
                    connection
                        .fri_basic_witness_generator_dal()
                        .check_reached_max_attempts(max_attempts)
                        .await,
                );
            }
            AggregationRound::LeafAggregation => {
                PROVER_JOB_MONITOR_METRICS.report_reached_max_attempts(
                    JobType::LeafWitnessGenerator,
                    connection
                        .fri_leaf_witness_generator_dal()
                        .check_reached_max_attempts(max_attempts)
                        .await,
                );
            }
            AggregationRound::NodeAggregation => {
                PROVER_JOB_MONITOR_METRICS.report_reached_max_attempts(
                    JobType::NodeWitnessGenerator,
                    connection
                        .fri_node_witness_generator_dal()
                        .check_reached_max_attempts(max_attempts)
                        .await,
                );
            }
            AggregationRound::RecursionTip => {
                PROVER_JOB_MONITOR_METRICS.report_reached_max_attempts(
                    JobType::RecursionTipWitnessGenerator,
                    connection
                        .fri_recursion_tip_witness_generator_dal()
                        .check_reached_max_attempts(max_attempts)
                        .await,
                );
            }
            AggregationRound::Scheduler => {
                PROVER_JOB_MONITOR_METRICS.report_reached_max_attempts(
                    JobType::SchedulerWitnessGenerator,
                    connection
                        .fri_scheduler_witness_generator_dal()
                        .check_reached_max_attempts(max_attempts)
                        .await,
                );
            }
        }

        Ok(())
    }

    pub async fn check_prover_job_attempts(
        &self,
        connection: &mut Connection<'_, Prover>,
    ) -> anyhow::Result<()> {
        let max_attempts = self.prover_config.max_attempts;

        PROVER_JOB_MONITOR_METRICS.report_reached_max_attempts(
            JobType::ProverFri,
            connection
                .fri_prover_jobs_dal()
                .check_reached_max_attempts(max_attempts)
                .await,
        );

        Ok(())
    }

    pub async fn check_proof_compressor_job_attempts(
        &self,
        connection: &mut Connection<'_, Prover>,
    ) -> anyhow::Result<()> {
        let max_attempts = self.compressor_config.max_attempts;

        PROVER_JOB_MONITOR_METRICS.report_reached_max_attempts(
            JobType::ProofCompressor,
            connection
                .fri_proof_compressor_dal()
                .check_reached_max_attempts(max_attempts)
                .await,
        );

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for ProverJobAttemptsReporter {
    async fn invoke(&self) -> anyhow::Result<()> {
        let mut connection = self
            .pool
            .connection()
            .await
            .context("failed to get database connection")?;
        self.check_witness_generator_job_attempts(&mut connection, AggregationRound::BasicCircuits)
            .await?;
        self.check_witness_generator_job_attempts(
            &mut connection,
            AggregationRound::LeafAggregation,
        )
        .await?;
        self.check_witness_generator_job_attempts(
            &mut connection,
            AggregationRound::NodeAggregation,
        )
        .await?;
        self.check_witness_generator_job_attempts(&mut connection, AggregationRound::RecursionTip)
            .await?;
        self.check_witness_generator_job_attempts(&mut connection, AggregationRound::Scheduler)
            .await?;

        self.check_prover_job_attempts(&mut connection).await?;

        self.check_proof_compressor_job_attempts(&mut connection)
            .await?;

        Ok(())
    }
}
