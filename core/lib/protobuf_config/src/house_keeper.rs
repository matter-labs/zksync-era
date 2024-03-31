use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::house_keeper as proto;

impl ProtoRepr for proto::HouseKeeper {
    type Type = configs::house_keeper::HouseKeeperConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            l1_batch_metrics_reporting_interval_ms: *required(
                &self.l1_batch_metrics_reporting_interval_ms,
            )
            .context("l1_batch_metrics_reporting_interval_ms")?,
            gpu_prover_queue_reporting_interval_ms: *required(
                &self.gpu_prover_queue_reporting_interval_ms,
            )
            .context("gpu_prover_queue_reporting_interval_ms")?,
            prover_job_retrying_interval_ms: *required(&self.prover_job_retrying_interval_ms)
                .context("prover_job_retrying_interval_ms")?,
            prover_stats_reporting_interval_ms: *required(&self.prover_stats_reporting_interval_ms)
                .context("prover_stats_reporting_interval_ms")?,
            witness_job_moving_interval_ms: *required(&self.witness_job_moving_interval_ms)
                .context("witness_job_moving_interval_ms")?,
            witness_generator_stats_reporting_interval_ms: *required(
                &self.witness_generator_stats_reporting_interval_ms,
            )
            .context("witness_generator_stats_reporting_interval_ms")?,
            fri_witness_job_moving_interval_ms: *required(&self.fri_witness_job_moving_interval_ms)
                .context("fri_witness_job_moving_interval_ms")?,
            fri_prover_job_retrying_interval_ms: *required(
                &self.fri_prover_job_retrying_interval_ms,
            )
            .context("fri_prover_job_retrying_interval_ms")?,
            fri_witness_generator_job_retrying_interval_ms: *required(
                &self.fri_witness_generator_job_retrying_interval_ms,
            )
            .context("fri_witness_generator_job_retrying_interval_ms")?,
            prover_db_pool_size: *required(&self.prover_db_pool_size)
                .context("prover_db_pool_size")?,
            fri_prover_stats_reporting_interval_ms: *required(
                &self.fri_prover_stats_reporting_interval_ms,
            )
            .context("fri_prover_stats_reporting_interval_ms")?,
            fri_proof_compressor_job_retrying_interval_ms: *required(
                &self.fri_proof_compressor_job_retrying_interval_ms,
            )
            .context("fri_proof_compressor_job_retrying_interval_ms")?,
            fri_proof_compressor_stats_reporting_interval_ms: *required(
                &self.fri_proof_compressor_stats_reporting_interval_ms,
            )
            .context("fri_proof_compressor_stats_reporting_interval_ms")?,
            // TODO(PLA-862): Make these 2 variables required
            fri_prover_job_archiver_reporting_interval_ms: self
                .fri_prover_job_archiver_reporting_interval_ms,
            fri_prover_job_archiver_archiving_interval_secs: self
                .fri_prover_job_archiver_archiving_interval_secs,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            l1_batch_metrics_reporting_interval_ms: Some(
                this.l1_batch_metrics_reporting_interval_ms,
            ),
            gpu_prover_queue_reporting_interval_ms: Some(
                this.gpu_prover_queue_reporting_interval_ms,
            ),
            prover_job_retrying_interval_ms: Some(this.prover_job_retrying_interval_ms),
            prover_stats_reporting_interval_ms: Some(this.prover_stats_reporting_interval_ms),
            witness_job_moving_interval_ms: Some(this.witness_job_moving_interval_ms),
            witness_generator_stats_reporting_interval_ms: Some(
                this.witness_generator_stats_reporting_interval_ms,
            ),
            fri_witness_job_moving_interval_ms: Some(this.fri_witness_job_moving_interval_ms),
            fri_prover_job_retrying_interval_ms: Some(this.fri_prover_job_retrying_interval_ms),
            fri_witness_generator_job_retrying_interval_ms: Some(
                this.fri_witness_generator_job_retrying_interval_ms,
            ),
            prover_db_pool_size: Some(this.prover_db_pool_size),
            fri_prover_stats_reporting_interval_ms: Some(
                this.fri_prover_stats_reporting_interval_ms,
            ),
            fri_proof_compressor_job_retrying_interval_ms: Some(
                this.fri_proof_compressor_job_retrying_interval_ms,
            ),
            fri_proof_compressor_stats_reporting_interval_ms: Some(
                this.fri_proof_compressor_stats_reporting_interval_ms,
            ),
            fri_prover_job_archiver_reporting_interval_ms: this
                .fri_prover_job_archiver_reporting_interval_ms,
            fri_prover_job_archiver_archiving_interval_secs: this
                .fri_prover_job_archiver_archiving_interval_secs,
        }
    }
}
