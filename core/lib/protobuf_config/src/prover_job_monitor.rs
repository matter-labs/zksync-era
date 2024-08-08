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
            max_db_connections: required(&self.max_db_connections)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_db_connections")?,
            graceful_shutdown_timeout_ms: required(&self.graceful_shutdown_timeout_ms)
                .and_then(|x| Ok((*x).try_into()?))
                .context("graceful_shutdown_timeout_ms")?,
            // l1_batch_metrics_reporting_interval_ms: *required(
            //     &self.l1_batch_metrics_reporting_interval_ms,
            // )
            //     .context("l1_batch_metrics_reporting_interval_ms")?,
            // gpu_prover_queue_reporting_interval_ms: *required(
            //     &self.gpu_prover_queue_reporting_interval_ms,
            // )
            //     .context("gpu_prover_queue_reporting_interval_ms")?,
            // prover_job_retrying_interval_ms: *required(&self.prover_job_retrying_interval_ms)
            //     .context("prover_job_retrying_interval_ms")?,
            // prover_stats_reporting_interval_ms: *required(&self.prover_stats_reporting_interval_ms)
            //     .context("prover_stats_reporting_interval_ms")?,
            // witness_job_moving_interval_ms: *required(&self.witness_job_moving_interval_ms)
            //     .context("witness_job_moving_interval_ms")?,
            // witness_generator_stats_reporting_interval_ms: *required(
            //     &self.witness_generator_stats_reporting_interval_ms,
            // )
            //     .context("witness_generator_stats_reporting_interval_ms")?,
            // prover_db_pool_size: *required(&self.prover_db_pool_size)
            //     .context("prover_db_pool_size")?,
            // proof_compressor_job_retrying_interval_ms: *required(
            //     &self.proof_compressor_job_retrying_interval_ms,
            // )
            //     .context("proof_compressor_job_retrying_interval_ms")?,
            // witness_generator_job_retrying_interval_ms: *required(
            //     &self.witness_generator_job_retrying_interval_ms,
            // )
            //     .context("witness_generator_job_retrying_interval_ms")?,
            // proof_compressor_stats_reporting_interval_ms: *required(
            //     &self.proof_compressor_stats_reporting_interval_ms,
            // )
            //     .context("proof_compressor_stats_reporting_interval_ms")?,
            //
            // // TODO(PLA-862): Make these 2 variables required
            // prover_job_archiver_archiving_interval_ms: self
            //     .prover_job_archiver_archiving_interval_ms,
            // prover_job_archiver_archive_after_secs: self.prover_job_archiver_archive_after_secs,
            // fri_gpu_prover_archiver_archiving_interval_ms: self
            //     .fri_gpu_prover_archiver_archiving_interval_ms,
            // fri_gpu_prover_archiver_archive_after_secs: self
            //     .fri_gpu_prover_archiver_archive_after_secs,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            prometheus_port: Some(this.prometheus_port.into()),
            max_db_connections: Some(this.max_db_connections.into()),
            graceful_shutdown_timeout_ms: Some(this.graceful_shutdown_timeout_ms.into()),
            // l1_batch_metrics_reporting_interval_ms: Some(
            //     this.l1_batch_metrics_reporting_interval_ms,
            // ),
            // gpu_prover_queue_reporting_interval_ms: Some(
            //     this.gpu_prover_queue_reporting_interval_ms,
            // ),
            // prover_job_retrying_interval_ms: Some(this.prover_job_retrying_interval_ms),
            // prover_stats_reporting_interval_ms: Some(this.prover_stats_reporting_interval_ms),
            // witness_job_moving_interval_ms: Some(this.witness_job_moving_interval_ms),
            // witness_generator_stats_reporting_interval_ms: Some(
            //     this.witness_generator_stats_reporting_interval_ms,
            // ),
            // witness_generator_job_retrying_interval_ms: Some(
            //     this.witness_generator_job_retrying_interval_ms,
            // ),
            // prover_db_pool_size: Some(this.prover_db_pool_size),
            // proof_compressor_job_retrying_interval_ms: Some(
            //     this.proof_compressor_job_retrying_interval_ms,
            // ),
            // proof_compressor_stats_reporting_interval_ms: Some(
            //     this.proof_compressor_stats_reporting_interval_ms,
            // ),
            // prover_job_archiver_archiving_interval_ms: this
            //     .prover_job_archiver_archiving_interval_ms,
            // prover_job_archiver_archive_after_secs: this.prover_job_archiver_archive_after_secs,
            // fri_gpu_prover_archiver_archiving_interval_ms: this
            //     .fri_gpu_prover_archiver_archiving_interval_ms,
            // fri_gpu_prover_archiver_archive_after_secs: this
            //     .fri_gpu_prover_archiver_archive_after_secs,
        }
    }
}
