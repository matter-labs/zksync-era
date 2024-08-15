use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::prover_job_monitor as proto;

impl ProtoRepr for proto::ProverJobMonitor {
    type Type = configs::prover_job_monitor::ProverJobMonitorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            prometheus_port: required(&self.prometheus_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_port")?,
            max_db_connections: *required(&self.max_db_connections)
                .context("max_db_connections")?,
            graceful_shutdown_timeout_ms: *required(&self.graceful_shutdown_timeout_ms)
                .context("graceful_shutdown_timeout_ms")?,
            gpu_prover_archiver_run_interval_ms: *required(
                &self.gpu_prover_archiver_run_interval_ms,
            )
            .context("gpu_prover_archiver_run_interval_ms")?,
            gpu_prover_archiver_archive_prover_after_secs: *required(
                &self.gpu_prover_archiver_archive_prover_after_secs,
            )
            .context("gpu_prover_archiver_archive_prover_after_secs")?,
            prover_jobs_archiver_run_interval_ms: *required(
                &self.prover_jobs_archiver_run_interval_ms,
            )
            .context("prover_jobs_archiver_run_interval_ms")?,
            prover_jobs_archiver_archive_jobs_after_secs: *required(
                &self.prover_jobs_archiver_archive_jobs_after_secs,
            )
            .context("prover_jobs_archiver_archive_jobs_after_secs")?,
            proof_compressor_job_requeuer_run_interval_ms: *required(
                &self.proof_compressor_job_requeuer_run_interval_ms,
            )
            .context("proof_compressor_job_requeuer_run_interval_ms")?,
            prover_job_requeuer_run_interval_ms: *required(
                &self.prover_job_requeuer_run_interval_ms,
            )
            .context("prover_job_requeuer_run_interval_ms")?,
            witness_generator_job_requeuer_run_interval_ms: *required(
                &self.witness_generator_job_requeuer_run_interval_ms,
            )
            .context("witness_generator_job_requeuer_run_interval_ms")?,
            proof_compressor_queue_reporter_run_interval_ms: *required(
                &self.proof_compressor_queue_reporter_run_interval_ms,
            )
            .context("proof_compressor_queue_reporter_run_interval_ms")?,
            prover_queue_reporter_run_interval_ms: *required(
                &self.prover_queue_reporter_run_interval_ms,
            )
            .context("prover_queue_reporter_run_interval_ms")?,
            witness_generator_queue_reporter_run_interval_ms: *required(
                &self.witness_generator_queue_reporter_run_interval_ms,
            )
            .context("witness_generator_queue_reporter_run_interval_ms")?,
            witness_job_queuer_run_interval_ms: *required(&self.witness_job_queuer_run_interval_ms)
                .context("witness_job_queuer_run_interval_ms")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            prometheus_port: Some(this.prometheus_port.into()),
            max_db_connections: Some(this.max_db_connections),
            graceful_shutdown_timeout_ms: Some(this.graceful_shutdown_timeout_ms),
            gpu_prover_archiver_run_interval_ms: Some(this.gpu_prover_archiver_run_interval_ms),
            gpu_prover_archiver_archive_prover_after_secs: Some(
                this.gpu_prover_archiver_archive_prover_after_secs,
            ),
            prover_jobs_archiver_run_interval_ms: Some(this.prover_jobs_archiver_run_interval_ms),
            prover_jobs_archiver_archive_jobs_after_secs: Some(
                this.prover_jobs_archiver_archive_jobs_after_secs,
            ),
            proof_compressor_job_requeuer_run_interval_ms: Some(
                this.proof_compressor_job_requeuer_run_interval_ms,
            ),
            prover_job_requeuer_run_interval_ms: Some(this.prover_job_requeuer_run_interval_ms),
            witness_generator_job_requeuer_run_interval_ms: Some(
                this.witness_generator_job_requeuer_run_interval_ms,
            ),
            proof_compressor_queue_reporter_run_interval_ms: Some(
                this.proof_compressor_queue_reporter_run_interval_ms,
            ),
            prover_queue_reporter_run_interval_ms: Some(this.prover_queue_reporter_run_interval_ms),
            witness_generator_queue_reporter_run_interval_ms: Some(
                this.witness_generator_queue_reporter_run_interval_ms,
            ),
            witness_job_queuer_run_interval_ms: Some(this.witness_job_queuer_run_interval_ms),
        }
    }
}
