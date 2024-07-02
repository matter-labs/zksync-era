use zksync_config::configs::{
    fri_prover_group::FriProverGroupConfig, house_keeper::HouseKeeperConfig,
    FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig,
};
use zksync_house_keeper::{
    blocks_state_reporter::L1BatchMetricsReporter,
    periodic_job::PeriodicJob,
    prover::{
        FriGpuProverArchiver, FriProofCompressorJobRetryManager, FriProofCompressorQueueReporter,
        FriProverJobRetryManager, FriProverJobsArchiver, FriProverQueueReporter,
        FriWitnessGeneratorJobRetryManager, FriWitnessGeneratorQueueReporter,
        WaitingToQueuedFriWitnessJobMover,
    },
};

use crate::{
    implementations::resources::pools::{PoolResource, ProverPool, ReplicaPool},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for `HouseKeeper` - a component responsible for managing prover jobs
/// and auxiliary server activities.
///
/// ## Requests resources
///
/// - `PoolResource<ReplicaPool>`
/// - `PoolResource<ProverPool>`
///
/// ## Adds tasks
///
/// - `L1BatchMetricsReporterTask`
/// - `FriProverJobRetryManagerTask`
/// - `FriWitnessGeneratorJobRetryManagerTask`
/// - `WaitingToQueuedFriWitnessJobMoverTask`
/// - `FriProverJobArchiverTask`
/// - `FriProverGpuArchiverTask`
/// - `FriWitnessGeneratorStatsReporterTask`
/// - `FriProverStatsReporterTask`
/// - `FriProofCompressorStatsReporterTask`
/// - `FriProofCompressorJobRetryManagerTask`
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
        // Initialize resources
        let replica_pool_resource = context.get_resource::<PoolResource<ReplicaPool>>()?;
        let replica_pool = replica_pool_resource.get().await?;

        let prover_pool_resource = context.get_resource::<PoolResource<ProverPool>>()?;
        let prover_pool = prover_pool_resource.get().await?;

        // Initialize and add tasks
        let l1_batch_metrics_reporter = L1BatchMetricsReporter::new(
            self.house_keeper_config
                .l1_batch_metrics_reporting_interval_ms,
            replica_pool.clone(),
        );
        context.add_task(L1BatchMetricsReporterTask {
            l1_batch_metrics_reporter,
        });

        let fri_prover_job_retry_manager = FriProverJobRetryManager::new(
            self.fri_prover_config.max_attempts,
            self.fri_prover_config.proof_generation_timeout(),
            self.house_keeper_config.prover_job_retrying_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(FriProverJobRetryManagerTask {
            fri_prover_job_retry_manager,
        });

        let fri_witness_gen_job_retry_manager = FriWitnessGeneratorJobRetryManager::new(
            self.fri_witness_generator_config.max_attempts,
            self.fri_witness_generator_config
                .witness_generation_timeouts(),
            self.house_keeper_config
                .witness_generator_job_retrying_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(FriWitnessGeneratorJobRetryManagerTask {
            fri_witness_gen_job_retry_manager,
        });

        let waiting_to_queued_fri_witness_job_mover = WaitingToQueuedFriWitnessJobMover::new(
            self.house_keeper_config.witness_job_moving_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(WaitingToQueuedFriWitnessJobMoverTask {
            waiting_to_queued_fri_witness_job_mover,
        });

        if let Some((archiving_interval, archive_after)) =
            self.house_keeper_config.prover_job_archiver_params()
        {
            let fri_prover_job_archiver =
                FriProverJobsArchiver::new(prover_pool.clone(), archiving_interval, archive_after);
            context.add_task(FriProverJobArchiverTask {
                fri_prover_job_archiver,
            });
        }

        if let Some((archiving_interval, archive_after)) =
            self.house_keeper_config.fri_gpu_prover_archiver_params()
        {
            let fri_prover_gpu_archiver =
                FriGpuProverArchiver::new(prover_pool.clone(), archiving_interval, archive_after);
            context.add_task(FriProverGpuArchiverTask {
                fri_prover_gpu_archiver,
            });
        }

        let fri_witness_generator_stats_reporter = FriWitnessGeneratorQueueReporter::new(
            prover_pool.clone(),
            self.house_keeper_config
                .witness_generator_stats_reporting_interval_ms,
        );
        context.add_task(FriWitnessGeneratorStatsReporterTask {
            fri_witness_generator_stats_reporter,
        });

        let fri_prover_stats_reporter = FriProverQueueReporter::new(
            self.house_keeper_config.prover_stats_reporting_interval_ms,
            prover_pool.clone(),
            replica_pool.clone(),
            self.fri_prover_group_config,
        );
        context.add_task(FriProverStatsReporterTask {
            fri_prover_stats_reporter,
        });

        let fri_proof_compressor_stats_reporter = FriProofCompressorQueueReporter::new(
            self.house_keeper_config
                .proof_compressor_stats_reporting_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(FriProofCompressorStatsReporterTask {
            fri_proof_compressor_stats_reporter,
        });

        let fri_proof_compressor_retry_manager = FriProofCompressorJobRetryManager::new(
            self.fri_proof_compressor_config.max_attempts,
            self.fri_proof_compressor_config.generation_timeout(),
            self.house_keeper_config
                .proof_compressor_job_retrying_interval_ms,
            prover_pool.clone(),
        );
        context.add_task(FriProofCompressorJobRetryManagerTask {
            fri_proof_compressor_retry_manager,
        });

        Ok(())
    }
}

#[derive(Debug)]
struct L1BatchMetricsReporterTask {
    l1_batch_metrics_reporter: L1BatchMetricsReporter,
}

#[async_trait::async_trait]
impl Task for L1BatchMetricsReporterTask {
    fn id(&self) -> TaskId {
        "l1_batch_metrics_reporter".into()
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
    fn id(&self) -> TaskId {
        "fri_prover_job_retry_manager".into()
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
    fn id(&self) -> TaskId {
        "fri_witness_generator_job_retry_manager".into()
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
    fn id(&self) -> TaskId {
        "waiting_to_queued_fri_witness_job_mover".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.waiting_to_queued_fri_witness_job_mover
            .run(stop_receiver.0)
            .await
    }
}

#[derive(Debug)]
struct FriWitnessGeneratorStatsReporterTask {
    fri_witness_generator_stats_reporter: FriWitnessGeneratorQueueReporter,
}

#[async_trait::async_trait]
impl Task for FriWitnessGeneratorStatsReporterTask {
    fn id(&self) -> TaskId {
        "fri_witness_generator_stats_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_witness_generator_stats_reporter
            .run(stop_receiver.0)
            .await
    }
}

#[derive(Debug)]
struct FriProverStatsReporterTask {
    fri_prover_stats_reporter: FriProverQueueReporter,
}

#[async_trait::async_trait]
impl Task for FriProverStatsReporterTask {
    fn id(&self) -> TaskId {
        "fri_prover_stats_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_prover_stats_reporter.run(stop_receiver.0).await
    }
}

#[derive(Debug)]
struct FriProofCompressorStatsReporterTask {
    fri_proof_compressor_stats_reporter: FriProofCompressorQueueReporter,
}

#[async_trait::async_trait]
impl Task for FriProofCompressorStatsReporterTask {
    fn id(&self) -> TaskId {
        "fri_proof_compressor_stats_reporter".into()
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
    fn id(&self) -> TaskId {
        "fri_proof_compressor_job_retry_manager".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_proof_compressor_retry_manager
            .run(stop_receiver.0)
            .await
    }
}

#[derive(Debug)]
struct FriProverJobArchiverTask {
    fri_prover_job_archiver: FriProverJobsArchiver,
}

#[async_trait::async_trait]
impl Task for FriProverJobArchiverTask {
    fn id(&self) -> TaskId {
        "fri_prover_job_archiver".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_prover_job_archiver.run(stop_receiver.0).await
    }
}

struct FriProverGpuArchiverTask {
    fri_prover_gpu_archiver: FriGpuProverArchiver,
}

#[async_trait::async_trait]
impl Task for FriProverGpuArchiverTask {
    fn id(&self) -> TaskId {
        "fri_prover_gpu_archiver".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.fri_prover_gpu_archiver.run(stop_receiver.0).await
    }
}
