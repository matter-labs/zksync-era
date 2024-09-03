use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use tokio::{sync::watch::Receiver, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_fri_types::ProverJob;
use zksync_types::protocol_version::ProtocolSemanticVersion;
use zksync_vk_setup_data_server_fri::keystore::Keystore;

pub struct WitnessVectorGenerator {
    object_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Prover>,
    protocol_version: ProtocolSemanticVersion,
    max_attempts: u32,
    keystore: Keystore,
}

impl WitnessVectorGenerator {
    const POLLING_INTERVAL_MS: u64 = 1500;

    pub fn new(
        object_store: Arc<dyn ObjectStore>,
        connection_pool: ConnectionPool<Prover>,
        protocol_version: ProtocolSemanticVersion,
        max_attempts: u32,
        setup_data_path: Option<String>,
    ) -> Self {
        let keystore = if let Some(setup_data_path) = setup_data_path {
            Keystore::new_with_setup_data_path(setup_data_path)
        } else {
            Keystore::default()
        };
        Self {
            object_store,
            connection_pool,
            protocol_version,
            max_attempts,
            keystore,
        }
    }

    pub async fn run(self, cancellation_token: CancellationToken) -> anyhow::Result<()> {
        tracing::info!("starting new run");
        let mut backoff: u64 = Self::POLLING_INTERVAL_MS;
        while !cancellation_token.is_cancelled() {
            tracing::info!("before getting new job");
            if let Some((job_id, job)) = self
                .get_next_job()
                .await
                .context("failed during get_next_job()")?
            {
                let started_at = Instant::now();
                backoff = Self::POLLING_INTERVAL_MS;
                // tracing::debug!(
                //                     "Spawning thread processing {:?} job with id {:?}",
                //                     Self::SERVICE_NAME,
                //                     job_id
                //                 );
                let task = self.process_job(&job_id, job, started_at).await;

                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        tracing::info!("received cancellation!");
                        return Ok(())
                    }
                    data = task => {
                        tracing::info!("finished task!");
                        // tracing::debug!(
                        //     "{} Job {:?} finished successfully",
                        //     Self::SERVICE_NAME,
                        //     job_id
                        // );
                        // // METRICS.attempts[&Self::SERVICE_NAME].observe(attempts as usize);
                        // return self
                        //     .save_result(job_id, started_at, data)
                        //     .await
                        //     .context("save_result()");
                    }
                }
                continue;
            };
            tracing::info!("Backing off for {} ms", backoff);
            // Error here corresponds to a timeout w/o receiving task cancel; we're OK with this.
            tokio::time::timeout(
                Duration::from_millis(backoff),
                cancellation_token.cancelled(),
            )
            .await
            .ok();
            backoff = (backoff * Self::BACKOFF_MULTIPLIER).min(Self::MAX_BACKOFF_MS);
        }
        tracing::warn!("Stop signal received, shutting down Witness Vector Generator");
        Ok(())

        // vvv wait for task
        //     /// Polls task handle, saving its outcome.
        //     async fn wait_for_task(
        //         &self,
        //         job_id: Self::JobId,
        //         started_at: Instant,
        //         task: JoinHandle<anyhow::Result<Self::JobArtifacts>>,
        //         stop_receiver: &mut watch::Receiver<bool>,
        //     ) -> anyhow::Result<()> {
        //         let attempts = self.get_job_attempts(&job_id).await?;
        //         let max_attempts = self.max_attempts();
        //         if attempts == max_attempts {
        //             METRICS.max_attempts_reached[&(Self::SERVICE_NAME, format!("{job_id:?}"))].inc();
        //             tracing::error!(
        //                 "Max attempts ({max_attempts}) reached for {} job {:?}",
        //                 Self::SERVICE_NAME,
        //                 job_id,
        //             );
        //         }
        //
        //         let result = loop {
        //             tracing::trace!(
        //                 "Polling {} task with id {:?}. Is finished: {}",
        //                 Self::SERVICE_NAME,
        //                 job_id,
        //                 task.is_finished()
        //             );
        //             if task.is_finished() {
        //                 break task.await;
        //             }
        //             if stop_receiver.changed().await.is_ok() {
        //                 return Ok(());
        //             }
        //         };
        //         let error_message = match result {
        //             Ok(Ok(data)) => {
        //                 tracing::debug!(
        //                     "{} Job {:?} finished successfully",
        //                     Self::SERVICE_NAME,
        //                     job_id
        //                 );
        //                 METRICS.attempts[&Self::SERVICE_NAME].observe(attempts as usize);
        //                 return self
        //                     .save_result(job_id, started_at, data)
        //                     .await
        //                     .context("save_result()");
        //             }
        //             Ok(Err(error)) => error.to_string(),
        //             Err(error) => try_extract_panic_message(error),
        //         };
        //         tracing::error!(
        //             "Error occurred while processing {} job {:?}: {:?}",
        //             Self::SERVICE_NAME,
        //             job_id,
        //             error_message
        //         );
        //
        //         self.save_failure(job_id, started_at, error_message).await;
        //         Ok(())
        //     }

        // ----------------------

        // while iterations_left.map_or(true, |i| i > 0) {
        //     if *stop_receiver.borrow() {
        //         tracing::warn!(
        //                     "Stop signal received, shutting down {} component while waiting for a new job",
        //                     Self::SERVICE_NAME
        //                 );
        //         return Ok(());
        //     }
        //     if let Some((job_id, job)) =
        //         Self::get_next_job(&self).await.context("get_next_job()")?
        //     {
        //         let started_at = Instant::now();
        //         backoff = Self::POLLING_INTERVAL_MS;
        //         iterations_left = iterations_left.map(|i| i - 1);
        //
        //         tracing::debug!(
        //                     "Spawning thread processing {:?} job with id {:?}",
        //                     Self::SERVICE_NAME,
        //                     job_id
        //                 );
        //         let task = self.process_job(&job_id, job, started_at).await;
        //
        //         self.wait_for_task(job_id, started_at, task, &mut stop_receiver)
        //             .await
        //             .context("wait_for_task")?;
        //     } else if iterations_left.is_some() {
        //         tracing::info!("No more jobs to process. Server can stop now.");
        //         return Ok(());
        //     } else {
        //         tracing::trace!("Backing off for {} ms", backoff);
        //         // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
        //         tokio::time::timeout(Duration::from_millis(backoff), stop_receiver.changed())
        //             .await
        //             .ok();
        //         backoff = (backoff * Self::BACKOFF_MULTIPLIER).min(Self::MAX_BACKOFF_MS);
        //     }
        // }
        // tracing::info!("Requested number of jobs is processed. Server can stop now.");
        // Ok(())
    }

    async fn get_next_job(&self) -> anyhow::Result<Option<(i32, i32)>> {
        // TODO: Fix this guy
        Ok(Some((1, 2)))
    }

    //     /// Function that processes a job
    async fn process_job(
        &self,
        job_id: &i32,
        job: i32,
        started_at: Instant,
    ) -> JoinHandle<anyhow::Result<()>> {
        tokio::task::spawn_blocking(|| {
            std::thread::sleep(Duration::from_secs(10));
            Ok(())
        })
    }
}
//     type Job = ProverJob;
//     type JobId = u32;

// vvv JobProcessor implementation

// use std::{
//     fmt::Debug,
//     time::{Duration, Instant},
// };
//
// use anyhow::Context as _;
// pub use async_trait::async_trait;
// use tokio::{sync::watch, task::JoinHandle};
// use vise::{Buckets, Counter, Histogram, LabeledFamily, Metrics};
// use zksync_utils::panic_extractor::try_extract_panic_message;
//
// const ATTEMPT_BUCKETS: Buckets = Buckets::exponential(1.0..=64.0, 2.0);
//
// #[derive(Debug, Metrics)]
// #[metrics(prefix = "job_processor")]
// struct JobProcessorMetrics {
//     #[metrics(labels = ["service_name", "job_id"])]
//     max_attempts_reached: LabeledFamily<(&'static str, String), Counter, 2>,
//     #[metrics(labels = ["service_name"], buckets = ATTEMPT_BUCKETS)]
//     attempts: LabeledFamily<&'static str, Histogram<usize>>,
// }
//
// #[vise::register]
// static METRICS: vise::Global<JobProcessorMetrics> = vise::Global::new();
//
// #[async_trait]
// pub trait JobProcessor: Sync + Send {
//     type Job: Send + 'static;
//     type JobId: Send + Sync + Debug + 'static;
//     type JobArtifacts: Send + 'static;
//
//     const POLLING_INTERVAL_MS: u64 = 1000;
//     const MAX_BACKOFF_MS: u64 = 60_000;
//     const BACKOFF_MULTIPLIER: u64 = 2;
//     const SERVICE_NAME: &'static str;
//
//     /// Returns None when there is no pending job
//     /// Otherwise, returns Some(job_id, job)
//     /// Note: must be concurrency-safe - that is, one job must not be returned in two parallel processes
//     async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>>;
//
//     /// Invoked when `process_job` panics
//     /// Should mark the job as failed
//     async fn save_failure(&self, job_id: Self::JobId, started_at: Instant, error: String);
//
//     /// Function that processes a job
//     async fn process_job(
//         &self,
//         job_id: &Self::JobId,
//         job: Self::Job,
//         started_at: Instant,
//     ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>>;
//
//     /// `iterations_left`:
//     /// To run indefinitely, pass `None`,
//     /// To process one job, pass `Some(1)`,
//     /// To process a batch, pass `Some(batch_size)`.
//     async fn run(
//         self,
//         mut stop_receiver: watch::Receiver<bool>,
//         mut iterations_left: Option<usize>,
//     ) -> anyhow::Result<()>
//     where
//         Self: Sized,
//     {
//         let mut backoff: u64 = Self::POLLING_INTERVAL_MS;
//         while iterations_left.map_or(true, |i| i > 0) {
//             if *stop_receiver.borrow() {
//                 tracing::warn!(
//                     "Stop signal received, shutting down {} component while waiting for a new job",
//                     Self::SERVICE_NAME
//                 );
//                 return Ok(());
//             }
//             if let Some((job_id, job)) =
//                 Self::get_next_job(&self).await.context("get_next_job()")?
//             {
//                 let started_at = Instant::now();
//                 backoff = Self::POLLING_INTERVAL_MS;
//                 iterations_left = iterations_left.map(|i| i - 1);
//
//                 tracing::debug!(
//                     "Spawning thread processing {:?} job with id {:?}",
//                     Self::SERVICE_NAME,
//                     job_id
//                 );
//                 let task = self.process_job(&job_id, job, started_at).await;
//
//                 self.wait_for_task(job_id, started_at, task, &mut stop_receiver)
//                     .await
//                     .context("wait_for_task")?;
//             } else if iterations_left.is_some() {
//                 tracing::info!("No more jobs to process. Server can stop now.");
//                 return Ok(());
//             } else {
//                 tracing::trace!("Backing off for {} ms", backoff);
//                 // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
//                 tokio::time::timeout(Duration::from_millis(backoff), stop_receiver.changed())
//                     .await
//                     .ok();
//                 backoff = (backoff * Self::BACKOFF_MULTIPLIER).min(Self::MAX_BACKOFF_MS);
//             }
//         }
//         tracing::info!("Requested number of jobs is processed. Server can stop now.");
//         Ok(())
//     }
//
//     /// Polls task handle, saving its outcome.
//     async fn wait_for_task(
//         &self,
//         job_id: Self::JobId,
//         started_at: Instant,
//         task: JoinHandle<anyhow::Result<Self::JobArtifacts>>,
//         stop_receiver: &mut watch::Receiver<bool>,
//     ) -> anyhow::Result<()> {
//         let attempts = self.get_job_attempts(&job_id).await?;
//         let max_attempts = self.max_attempts();
//         if attempts == max_attempts {
//             METRICS.max_attempts_reached[&(Self::SERVICE_NAME, format!("{job_id:?}"))].inc();
//             tracing::error!(
//                 "Max attempts ({max_attempts}) reached for {} job {:?}",
//                 Self::SERVICE_NAME,
//                 job_id,
//             );
//         }
//
//         let result = loop {
//             tracing::trace!(
//                 "Polling {} task with id {:?}. Is finished: {}",
//                 Self::SERVICE_NAME,
//                 job_id,
//                 task.is_finished()
//             );
//             if task.is_finished() {
//                 break task.await;
//             }
//             if stop_receiver.changed().await.is_ok() {
//                 return Ok(());
//             }
//         };
//         let error_message = match result {
//             Ok(Ok(data)) => {
//                 tracing::debug!(
//                     "{} Job {:?} finished successfully",
//                     Self::SERVICE_NAME,
//                     job_id
//                 );
//                 METRICS.attempts[&Self::SERVICE_NAME].observe(attempts as usize);
//                 return self
//                     .save_result(job_id, started_at, data)
//                     .await
//                     .context("save_result()");
//             }
//             Ok(Err(error)) => error.to_string(),
//             Err(error) => try_extract_panic_message(error),
//         };
//         tracing::error!(
//             "Error occurred while processing {} job {:?}: {:?}",
//             Self::SERVICE_NAME,
//             job_id,
//             error_message
//         );
//
//         self.save_failure(job_id, started_at, error_message).await;
//         Ok(())
//     }
//
//     /// Invoked when `process_job` doesn't panic
//     async fn save_result(
//         &self,
//         job_id: Self::JobId,
//         started_at: Instant,
//         artifacts: Self::JobArtifacts,
//     ) -> anyhow::Result<()>;
//
//     fn max_attempts(&self) -> u32;
//
//     /// Invoked in `wait_for_task` for in-progress job.
//     async fn get_job_attempts(&self, job_id: &Self::JobId) -> anyhow::Result<u32>;
// }

// vvv WVG IMPLEMENTATION

// use std::{
//     net::SocketAddr,
//     sync::Arc,
//     time::{Duration, Instant},
// };
//
// use anyhow::Context as _;
// use async_trait::async_trait;
// use tokio::{task::JoinHandle, time::sleep};
// use zksync_config::configs::FriWitnessVectorGeneratorConfig;
// use zksync_object_store::ObjectStore;
// use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
// use zksync_prover_fri_types::{
//     circuit_definitions::boojum::field::goldilocks::GoldilocksField, CircuitWrapper, ProverJob,
//     WitnessVectorArtifacts,
// };
// use zksync_prover_fri_utils::{
//     fetch_next_circuit, get_numeric_circuit_id, region_fetcher::Zone, socket_utils::send_assembly,
// };
// use zksync_queued_job_processor::JobProcessor;
// use zksync_types::{
//     basic_fri_types::CircuitIdRoundTuple, protocol_version::ProtocolSemanticVersion,
//     prover_dal::GpuProverInstanceStatus,
// };
// use zksync_vk_setup_data_server_fri::keystore::Keystore;
//
// // use crate::metrics::METRICS;
//
// pub struct WitnessVectorGenerator {
//     object_store: Arc<dyn ObjectStore>,
//     pool: ConnectionPool<Prover>,
//     circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
//     zone: Zone,
//     config: FriWitnessVectorGeneratorConfig,
//     protocol_version: ProtocolSemanticVersion,
//     max_attempts: u32,
//     setup_data_path: Option<String>,
// }
//
// impl WitnessVectorGenerator {
//     #[allow(clippy::too_many_arguments)]
//     pub fn new(
//         object_store: Arc<dyn ObjectStore>,
//         prover_connection_pool: ConnectionPool<Prover>,
//         circuit_ids_for_round_to_be_proven: Vec<CircuitIdRoundTuple>,
//         zone: Zone,
//         config: FriWitnessVectorGeneratorConfig,
//         protocol_version: ProtocolSemanticVersion,
//         max_attempts: u32,
//         setup_data_path: Option<String>,
//     ) -> Self {
//         Self {
//             object_store,
//             pool: prover_connection_pool,
//             circuit_ids_for_round_to_be_proven,
//             zone,
//             config,
//             protocol_version,
//             max_attempts,
//             setup_data_path,
//         }
//     }
//
//     #[tracing::instrument(
//         skip_all,
//         fields(l1_batch = % job.block_number)
//     )]
//     pub fn generate_witness_vector(
//         job: ProverJob,
//         keystore: &Keystore,
//     ) -> anyhow::Result<WitnessVectorArtifacts> {
//         let finalization_hints = keystore
//             .load_finalization_hints(job.setup_data_key.clone())
//             .context("get_finalization_hints()")?;
//         let cs = match job.circuit_wrapper.clone() {
//             CircuitWrapper::Base(base_circuit) => {
//                 base_circuit.synthesis::<GoldilocksField>(&finalization_hints)
//             }
//             CircuitWrapper::Recursive(recursive_circuit) => {
//                 recursive_circuit.synthesis::<GoldilocksField>(&finalization_hints)
//             }
//             CircuitWrapper::BasePartial(_) => {
//                 panic!("Invalid circuit wrapper received for witness vector generation");
//             }
//         };
//         Ok(WitnessVectorArtifacts::new(cs.witness.unwrap(), job))
//     }
// }
//
// #[async_trait]
// impl JobProcessor for WitnessVectorGenerator {
//     type Job = ProverJob;
//     type JobId = u32;
//     type JobArtifacts = WitnessVectorArtifacts;
//
//     const POLLING_INTERVAL_MS: u64 = 15000;
//     const SERVICE_NAME: &'static str = "WitnessVectorGenerator";
//
//     async fn get_next_job(&self) -> anyhow::Result<Option<(Self::JobId, Self::Job)>> {
//         let mut storage = self.pool.connection().await.unwrap();
//         let Some(job) = fetch_next_circuit(
//             &mut storage,
//             &*self.object_store,
//             &self.circuit_ids_for_round_to_be_proven,
//             &self.protocol_version,
//         )
//         .await
//         else {
//             return Ok(None);
//         };
//         Ok(Some((job.job_id, job)))
//     }
//
//     async fn save_failure(&self, job_id: Self::JobId, _started_at: Instant, error: String) {
//         self.pool
//             .connection()
//             .await
//             .unwrap()
//             .fri_prover_jobs_dal()
//             .save_proof_error(job_id, error)
//             .await;
//     }
//
//     async fn process_job(
//         &self,
//         _job_id: &Self::JobId,
//         job: ProverJob,
//         _started_at: Instant,
//     ) -> JoinHandle<anyhow::Result<Self::JobArtifacts>> {
//         let setup_data_path = self.setup_data_path.clone();
//
//         tokio::task::spawn_blocking(move || {
//             let block_number = job.block_number;
//             let _span = tracing::info_span!("witness_vector_generator", %block_number).entered();
//             let keystore = if let Some(setup_data_path) = setup_data_path {
//                 Keystore::new_with_setup_data_path(setup_data_path)
//             } else {
//                 Keystore::default()
//             };
//             Self::generate_witness_vector(job, &keystore)
//         })
//     }
//
//     #[tracing::instrument(
//         name = "WitnessVectorGenerator::save_result",
//         skip_all,
//         fields(l1_batch = % artifacts.prover_job.block_number)
//     )]
//     async fn save_result(
//         &self,
//         job_id: Self::JobId,
//         started_at: Instant,
//         artifacts: WitnessVectorArtifacts,
//     ) -> anyhow::Result<()> {
//         // let circuit_type =
//         //     get_numeric_circuit_id(&artifacts.prover_job.circuit_wrapper).to_string();
//
//         // METRICS.gpu_witness_vector_generation_time[&circuit_type].observe(started_at.elapsed());
//
//         tracing::info!(
//             "Finished witness vector generation for job: {job_id} in zone: {:?} took: {:?}",
//             self.zone,
//             started_at.elapsed()
//         );
//
//         let serialized: Vec<u8> =
//             bincode::serialize(&artifacts).expect("Failed to serialize witness vector artifacts");
//
//         let now = Instant::now();
//         let mut attempts = 0;
//
//         while now.elapsed() < self.config.prover_instance_wait_timeout() {
//             let prover = self
//                 .pool
//                 .connection()
//                 .await
//                 .unwrap()
//                 .fri_gpu_prover_queue_dal()
//                 .lock_available_prover(
//                     self.config.max_prover_reservation_duration(),
//                     self.config.specialized_group_id,
//                     self.zone.to_string(),
//                     self.protocol_version,
//                 )
//                 .await;
//
//             if let Some(address) = prover {
//                 let address = SocketAddr::from(address);
//                 tracing::info!(
//                     "Found prover at address {address:?} after {:?}. Sending witness vector job...",
//                     now.elapsed()
//                 );
//                 let result = send_assembly(job_id, &serialized, &address);
//                 handle_send_result(&result, job_id, &address, &self.pool, self.zone.to_string())
//                     .await;
//
//                 if result.is_ok() {
//                     // METRICS.prover_waiting_time[&circuit_type].observe(now.elapsed());
//                     // METRICS.prover_attempts_count[&circuit_type].observe(attempts as usize);
//                     tracing::info!(
//                         "Sent witness vector job to prover after {:?}",
//                         now.elapsed()
//                     );
//                     return Ok(());
//                 }
//
//                 tracing::warn!(
//                     "Could not send witness vector to {address:?}. Prover group {}, zone {}, \
//                          job {job_id}, send attempt {attempts}.",
//                     self.config.specialized_group_id,
//                     self.zone,
//                 );
//                 attempts += 1;
//             } else {
//                 tracing::warn!(
//                     "Could not find available prover. Time elapsed: {:?}. Will sleep for {:?}",
//                     now.elapsed(),
//                     self.config.prover_instance_poll_time()
//                 );
//                 sleep(self.config.prover_instance_poll_time()).await;
//             }
//         }
//         tracing::warn!(
//             "Not able to get any free prover instance for sending witness vector for job: {job_id} after {:?}", now.elapsed()
//         );
//         Ok(())
//     }
//
//     fn max_attempts(&self) -> u32 {
//         self.max_attempts
//     }
//
//     async fn get_job_attempts(&self, job_id: &u32) -> anyhow::Result<u32> {
//         let mut prover_storage = self
//             .pool
//             .connection()
//             .await
//             .context("failed to acquire DB connection for WitnessVectorGenerator")?;
//         prover_storage
//             .fri_prover_jobs_dal()
//             .get_prover_job_attempts(*job_id)
//             .await
//             .map(|attempts| attempts.unwrap_or(0))
//             .context("failed to get job attempts for WitnessVectorGenerator")
//     }
// }
//
// async fn handle_send_result(
//     result: &Result<(Duration, u64), String>,
//     job_id: u32,
//     address: &SocketAddr,
//     pool: &ConnectionPool<Prover>,
//     zone: String,
// ) {
//     match result {
//         Ok((elapsed, len)) => {
//             let blob_size_in_mb = len / (1024 * 1024);
//
//             tracing::info!(
//                 "Sent assembly of size: {blob_size_in_mb}MB successfully, took: {elapsed:?} \
//                  for job: {job_id} to: {address:?}"
//             );
//
//             // METRICS.blob_sending_time[&blob_size_in_mb.to_string()].observe(*elapsed);
//
//             pool.connection()
//                 .await
//                 .unwrap()
//                 .fri_prover_jobs_dal()
//                 .update_status(job_id, "in_gpu_proof")
//                 .await;
//         }
//
//         Err(err) => {
//             tracing::warn!(
//                 "Failed sending assembly to address: {address:?}, socket not reachable \
//                  reason: {err}"
//             );
//
//             // mark prover instance in `gpu_prover_queue` dead
//             pool.connection()
//                 .await
//                 .unwrap()
//                 .fri_gpu_prover_queue_dal()
//                 .update_prover_instance_status(
//                     (*address).into(),
//                     GpuProverInstanceStatus::Dead,
//                     zone,
//                 )
//                 .await;
//
//             // mark the job as failed
//             pool.connection()
//                 .await
//                 .unwrap()
//                 .fri_prover_jobs_dal()
//                 .save_proof_error(job_id, "prover instance unreachable".to_string())
//                 .await;
//         }
//     }
// }
