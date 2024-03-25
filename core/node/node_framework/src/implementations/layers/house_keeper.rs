use std::time::Duration;

use zksync_config::configs::{
    fri_prover_group::FriProverGroupConfig, house_keeper::HouseKeeperConfig,
    FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig,
};
use zksync_core::house_keeper::{
    blocks_state_reporter::L1BatchMetricsReporter,
    fri_proof_compressor_job_retry_manager::FriProofCompressorJobRetryManager,
    fri_proof_compressor_queue_monitor::FriProofCompressorStatsReporter,
    fri_prover_job_retry_manager::FriProverJobRetryManager,
    fri_prover_queue_monitor::FriProverStatsReporter,
    fri_scheduler_circuit_queuer::SchedulerCircuitQueuer,
    fri_witness_generator_jobs_retry_manager::FriWitnessGeneratorJobRetryManager,
    fri_witness_generator_queue_monitor::FriWitnessGeneratorStatsReporter,
    periodic_job::PeriodicJob,
    waiting_to_queued_fri_witness_job_mover::WaitingToQueuedFriWitnessJobMover,
};
use zksync_dal::{metrics::PostgresMetrics, ConnectionPool, Core};

use crate::{
    implementations::resources::pools::{ProverPoolResource, ReplicaPoolResource},
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

const SCRAPE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct HouseKeeperLayer {
    house_keeper_config: HouseKeeperConfig,
    fri_prover_config: FriProverConfig,
    fri_witness_generator_config: FriWitnessGeneratorConfig,
    fri_prover_group_config: FriProverGroupConfig,
    fri_proof_compressor_config: FriProofCompressorConfig,
}

impl HouseKeeperLayer {
    pub fn new(
        house_keeper_config: HouseKeeperConfig,
        fri_prover_config: FriProverConfig,
        fri_witness_generator_config: FriWitnessGeneratorConfig,
        fri_prover_group_config: FriProverGroupConfig,
        fri_proof_compressor_config: FriProofCompressorConfig,
    ) -> Self {
        Self {
            house_keeper_config,
            fri_prover_config,
            fri_witness_generator_config,
            fri_prover_group_config,
            fri_proof_compressor_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for HouseKeeperLayer {
    fn layer_name(&self) -> &'static str {
        "house_keeper_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // initialize resources
        let replica_pool_resource = context.get_resource::<ReplicaPoolResource>().await?;
        let replica_pool = replica_pool_resource.get().await?;

        let prover_pool_resource = context.get_resource::<ProverPoolResource>().await?;
        let prover_pool = prover_pool_resource.get().await?;

        // initialize and add tasks
        let pool_for_metrics = replica_pool.clone();
        context.add_task(Box::new(PoolForMetricsTask { pool_for_metrics }));

        let l1_batch_metrics_reporter = L1BatchMetricsReporter::new(
            self.house_keeper_config
                .l1_batch_metrics_reporting_interval_ms,
            replica_pool.clone(),
        );
        context.add_task(Box::new(L1BatchMetricsReporterTask {
            l1_batch_metrics_reporter,
        }));

        let fri_prover_job_retry_manager = FriProverJobRetryManager::new(
            self.fri_prover_config.max_attempts,
            self.fri_prover_config.proof_generation_timeout(),
            self.house_keeper_config.fri_prover_job_retrying_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(Box::new(FriProverJobRetryManagerTask {
            fri_prover_job_retry_manager,
        }));

        let fri_witness_gen_job_retry_manager = FriWitnessGeneratorJobRetryManager::new(
            self.fri_witness_generator_config.max_attempts,
            self.fri_witness_generator_config
                .witness_generation_timeout(),
            self.house_keeper_config
                .fri_witness_generator_job_retrying_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(Box::new(FriWitnessGeneratorJobRetryManagerTask {
            fri_witness_gen_job_retry_manager,
        }));

        let waiting_to_queued_fri_witness_job_mover = WaitingToQueuedFriWitnessJobMover::new(
            self.house_keeper_config.fri_witness_job_moving_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(Box::new(WaitingToQueuedFriWitnessJobMoverTask {
            waiting_to_queued_fri_witness_job_mover,
        }));

        let scheduler_circuit_queuer = SchedulerCircuitQueuer::new(
            self.house_keeper_config.fri_witness_job_moving_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(Box::new(SchedulerCircuitQueuerTask {
            scheduler_circuit_queuer,
        }));

        let fri_witness_generator_stats_reporter = FriWitnessGeneratorStatsReporter::new(
            prover_pool.clone(),
            self.house_keeper_config
                .witness_generator_stats_reporting_interval_ms,
        );
        context.add_task(Box::new(FriWitnessGeneratorStatsReporterTask {
            fri_witness_generator_stats_reporter,
        }));

        let fri_prover_stats_reporter = FriProverStatsReporter::new(
            self.house_keeper_config
                .fri_prover_stats_reporting_interval_ms,
            prover_pool.clone(),
            replica_pool.clone(),
            self.fri_prover_group_config,
        );
        context.add_task(Box::new(FriProverStatsReporterTask {
            fri_prover_stats_reporter,
        }));

        let fri_proof_compressor_stats_reporter = FriProofCompressorStatsReporter::new(
            self.house_keeper_config
                .fri_proof_compressor_stats_reporting_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(Box::new(FriProofCompressorStatsReporterTask {
            fri_proof_compressor_stats_reporter,
        }));

        let fri_proof_compressor_retry_manager = FriProofCompressorJobRetryManager::new(
            self.fri_proof_compressor_config.max_attempts,
            self.fri_proof_compressor_config.generation_timeout(),
            self.house_keeper_config
                .fri_proof_compressor_job_retrying_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(Box::new(FriProofCompressorJobRetryManagerTask {
            fri_proof_compressor_retry_manager,
        }));

        Ok(())
    }
}

#[derive(Debug)]
struct PoolForMetricsTask {
    pool_for_metrics: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl Task for PoolForMetricsTask {
    fn name(&self) -> &'static str {
        "pool_for_metrics"
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        PostgresMetrics::run_scraping(self.pool_for_metrics, SCRAPE_INTERVAL).await;
        Ok(())
    }
}

#[derive(Debug)]
struct L1BatchMetricsReporterTask {
    l1_batch_metrics_reporter: L1BatchMetricsReporter,
}

#[async_trait::async_trait]
impl Task for L1BatchMetricsReporterTask {
    fn name(&self) -> &'static str {
        "l1_batch_metrics_reporter"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.l1_batch_metrics_reporter.run(stop_receiver.0).await
    }
}

#[derive(Debug)]
struct FriProverJobRetryManagerTask {
    fri_prover_job_retry_manager: FriProverJobRetryManager,
}

#[async_trait::async_trait]
impl Task for FriProverJobRetryManagerTask {
    fn name(&self) -> &'static str {
        "fri_prover_job_retry_manager"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_prover_job_retry_manager.run(stop_receiver.0).await
    }
}

#[derive(Debug)]
struct FriWitnessGeneratorJobRetryManagerTask {
    fri_witness_gen_job_retry_manager: FriWitnessGeneratorJobRetryManager,
}

#[async_trait::async_trait]
impl Task for FriWitnessGeneratorJobRetryManagerTask {
    fn name(&self) -> &'static str {
        "fri_witness_generator_job_retry_manager"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_witness_gen_job_retry_manager
            .run(stop_receiver.0)
            .await
    }
}

#[derive(Debug)]
struct WaitingToQueuedFriWitnessJobMoverTask {
    waiting_to_queued_fri_witness_job_mover: WaitingToQueuedFriWitnessJobMover,
}

#[async_trait::async_trait]
impl Task for WaitingToQueuedFriWitnessJobMoverTask {
    fn name(&self) -> &'static str {
        "waiting_to_queued_fri_witness_job_mover"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.waiting_to_queued_fri_witness_job_mover
            .run(stop_receiver.0)
            .await
    }
}

#[derive(Debug)]
struct SchedulerCircuitQueuerTask {
    scheduler_circuit_queuer: SchedulerCircuitQueuer,
}

#[async_trait::async_trait]
impl Task for SchedulerCircuitQueuerTask {
    fn name(&self) -> &'static str {
        "scheduler_circuit_queuer"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.scheduler_circuit_queuer.run(stop_receiver.0).await
    }
}

#[derive(Debug)]
struct FriWitnessGeneratorStatsReporterTask {
    fri_witness_generator_stats_reporter: FriWitnessGeneratorStatsReporter,
}

#[async_trait::async_trait]
impl Task for FriWitnessGeneratorStatsReporterTask {
    fn name(&self) -> &'static str {
        "fri_witness_generator_stats_reporter"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_witness_generator_stats_reporter
            .run(stop_receiver.0)
            .await
    }
}

#[derive(Debug)]
struct FriProverStatsReporterTask {
    fri_prover_stats_reporter: FriProverStatsReporter,
}

#[async_trait::async_trait]
impl Task for FriProverStatsReporterTask {
    fn name(&self) -> &'static str {
        "fri_prover_stats_reporter"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_prover_stats_reporter.run(stop_receiver.0).await
    }
}

#[derive(Debug)]
struct FriProofCompressorStatsReporterTask {
    fri_proof_compressor_stats_reporter: FriProofCompressorStatsReporter,
}

#[async_trait::async_trait]
impl Task for FriProofCompressorStatsReporterTask {
    fn name(&self) -> &'static str {
        "fri_proof_compressor_stats_reporter"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_proof_compressor_stats_reporter
            .run(stop_receiver.0)
            .await
    }
}

#[derive(Debug)]
struct FriProofCompressorJobRetryManagerTask {
    fri_proof_compressor_retry_manager: FriProofCompressorJobRetryManager,
}

#[async_trait::async_trait]
impl Task for FriProofCompressorJobRetryManagerTask {
    fn name(&self) -> &'static str {
        "fri_proof_compressor_job_retry_manager"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_proof_compressor_retry_manager
            .run(stop_receiver.0)
            .await
    }
}
