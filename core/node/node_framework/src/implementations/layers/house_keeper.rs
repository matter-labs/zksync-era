use zksync_config::configs::{
    fri_prover_group::FriProverGroupConfig, house_keeper::HouseKeeperConfig,
    FriProofCompressorConfig, FriProverConfig, FriWitnessGeneratorConfig,
};
use zksync_house_keeper::{
    blocks_state_reporter::L1BatchMetricsReporter,
    prover::{
        FriGpuProverArchiver, FriProofCompressorJobRetryManager, FriProofCompressorQueueReporter,
        FriProverJobRetryManager, FriProverJobsArchiver, FriProverQueueReporter,
        FriWitnessGeneratorJobRetryManager, FriWitnessGeneratorQueueReporter,
        WaitingToQueuedFriWitnessJobMover,
    },
};
use zksync_periodic_job::PeriodicJob;

use crate::{
    implementations::resources::pools::{PoolResource, ProverPool, ReplicaPool},
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for `HouseKeeper` - a component responsible for managing prover jobs
/// and auxiliary server activities.
#[derive(Debug)]
pub struct HouseKeeperLayer {
    house_keeper_config: HouseKeeperConfig,
    fri_prover_config: FriProverConfig,
    fri_witness_generator_config: FriWitnessGeneratorConfig,
    fri_prover_group_config: FriProverGroupConfig,
    fri_proof_compressor_config: FriProofCompressorConfig,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub replica_pool: PoolResource<ReplicaPool>,
    pub prover_pool: PoolResource<ProverPool>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub l1_batch_metrics_reporter: L1BatchMetricsReporter,
    #[context(task)]
    pub fri_prover_job_retry_manager: FriProverJobRetryManager,
    #[context(task)]
    pub fri_witness_generator_job_retry_manager: FriWitnessGeneratorJobRetryManager,
    #[context(task)]
    pub waiting_to_queued_fri_witness_job_mover: WaitingToQueuedFriWitnessJobMover,
    #[context(task)]
    pub fri_prover_job_archiver: Option<FriProverJobsArchiver>,
    #[context(task)]
    pub fri_prover_gpu_archiver: Option<FriGpuProverArchiver>,
    #[context(task)]
    pub fri_witness_generator_stats_reporter: FriWitnessGeneratorQueueReporter,
    #[context(task)]
    pub fri_prover_stats_reporter: FriProverQueueReporter,
    #[context(task)]
    pub fri_proof_compressor_stats_reporter: FriProofCompressorQueueReporter,
    #[context(task)]
    pub fri_proof_compressor_job_retry_manager: FriProofCompressorJobRetryManager,
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
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "house_keeper_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // Initialize resources
        let replica_pool = input.replica_pool.get().await?;
        let prover_pool = input.prover_pool.get().await?;

        // Initialize and add tasks
        let l1_batch_metrics_reporter = L1BatchMetricsReporter::new(
            self.house_keeper_config
                .l1_batch_metrics_reporting_interval_ms,
            replica_pool.clone(),
        );

        let fri_prover_job_retry_manager = FriProverJobRetryManager::new(
            self.fri_prover_config.max_attempts,
            self.fri_prover_config.proof_generation_timeout(),
            self.house_keeper_config.prover_job_retrying_interval_ms,
            prover_pool.clone(),
        );

        let fri_witness_gen_job_retry_manager = FriWitnessGeneratorJobRetryManager::new(
            self.fri_witness_generator_config.max_attempts,
            self.fri_witness_generator_config
                .witness_generation_timeouts(),
            self.house_keeper_config
                .witness_generator_job_retrying_interval_ms,
            prover_pool.clone(),
        );

        let waiting_to_queued_fri_witness_job_mover = WaitingToQueuedFriWitnessJobMover::new(
            self.house_keeper_config.witness_job_moving_interval_ms,
            prover_pool.clone(),
        );

        let fri_prover_job_archiver = self.house_keeper_config.prover_job_archiver_params().map(
            |(archiving_interval, archive_after)| {
                FriProverJobsArchiver::new(prover_pool.clone(), archiving_interval, archive_after)
            },
        );

        let fri_prover_gpu_archiver = self
            .house_keeper_config
            .fri_gpu_prover_archiver_params()
            .map(|(archiving_interval, archive_after)| {
                FriGpuProverArchiver::new(prover_pool.clone(), archiving_interval, archive_after)
            });

        let fri_witness_generator_stats_reporter = FriWitnessGeneratorQueueReporter::new(
            prover_pool.clone(),
            self.house_keeper_config
                .witness_generator_stats_reporting_interval_ms,
        );

        let fri_prover_stats_reporter = FriProverQueueReporter::new(
            self.house_keeper_config.prover_stats_reporting_interval_ms,
            prover_pool.clone(),
            replica_pool.clone(),
            self.fri_prover_group_config,
        );

        let fri_proof_compressor_stats_reporter = FriProofCompressorQueueReporter::new(
            self.house_keeper_config
                .proof_compressor_stats_reporting_interval_ms,
            prover_pool.clone(),
        );

        let fri_proof_compressor_retry_manager = FriProofCompressorJobRetryManager::new(
            self.fri_proof_compressor_config.max_attempts,
            self.fri_proof_compressor_config.generation_timeout(),
            self.house_keeper_config
                .proof_compressor_job_retrying_interval_ms,
            prover_pool.clone(),
        );

        Ok(Output {
            l1_batch_metrics_reporter,
            fri_prover_job_retry_manager,
            fri_witness_generator_job_retry_manager: fri_witness_gen_job_retry_manager,
            waiting_to_queued_fri_witness_job_mover,
            fri_prover_job_archiver,
            fri_prover_gpu_archiver,
            fri_witness_generator_stats_reporter,
            fri_prover_stats_reporter,
            fri_proof_compressor_stats_reporter,
            fri_proof_compressor_job_retry_manager: fri_proof_compressor_retry_manager,
        })
    }
}

#[async_trait::async_trait]
impl Task for L1BatchMetricsReporter {
    fn id(&self) -> TaskId {
        "l1_batch_metrics_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for FriProverJobRetryManager {
    fn id(&self) -> TaskId {
        "fri_prover_job_retry_manager".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for FriWitnessGeneratorJobRetryManager {
    fn id(&self) -> TaskId {
        "fri_witness_generator_job_retry_manager".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for WaitingToQueuedFriWitnessJobMover {
    fn id(&self) -> TaskId {
        "waiting_to_queued_fri_witness_job_mover".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for FriWitnessGeneratorQueueReporter {
    fn id(&self) -> TaskId {
        "fri_witness_generator_queue_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for FriProverQueueReporter {
    fn id(&self) -> TaskId {
        "fri_prover_queue_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for FriProofCompressorQueueReporter {
    fn id(&self) -> TaskId {
        "fri_proof_compressor_queue_reporter".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for FriProofCompressorJobRetryManager {
    fn id(&self) -> TaskId {
        "fri_proof_compressor_job_retry_manager".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for FriProverJobsArchiver {
    fn id(&self) -> TaskId {
        "fri_prover_jobs_archiver".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait::async_trait]
impl Task for FriGpuProverArchiver {
    fn id(&self) -> TaskId {
        "fri_gpu_prover_archiver".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
