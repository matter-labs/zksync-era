//! Metrics for `MetadataCalculator`.

use std::time::Instant;

use zksync_config::configs::database::MerkleTreeMode;
use zksync_types::block::L1BatchHeader;
use zksync_utils::time::seconds_since_epoch;

use super::MetadataCalculator;

/// Stage of [`MetadataCalculator`] update reported via metric and logged.
pub(super) trait ReportStage: Copy {
    /// Name of the histogram using which the stage latency is reported.
    const HISTOGRAM_NAME: &'static str;

    /// Returns the stage tag for the histogram.
    fn as_tag(self) -> &'static str;

    /// Starts the stage.
    fn start(self) -> UpdateTreeLatency<Self> {
        UpdateTreeLatency {
            stage: self,
            start: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TreeUpdateStage {
    LoadChanges,
    Compute,
    PrepareResults,
    ReestimateGasCost,
    SavePostgres,
    SaveRocksDB,
    SaveWitnesses,
    _Backup,
}

impl ReportStage for TreeUpdateStage {
    const HISTOGRAM_NAME: &'static str = "server.metadata_calculator.update_tree.latency.stage";

    fn as_tag(self) -> &'static str {
        match self {
            Self::LoadChanges => "load_changes",
            Self::Compute => "compute",
            Self::PrepareResults => "prepare_results",
            Self::ReestimateGasCost => "reestimate_block_commit_gas_cost",
            Self::SavePostgres => "save_postgres",
            Self::SaveRocksDB => "save_rocksdb",
            Self::SaveWitnesses => "save_gcs",
            Self::_Backup => "backup_tree",
        }
    }
}

/// Sub-stages of [`TreeUpdateStage::LoadChanges`].
#[derive(Debug, Clone, Copy)]
pub(super) enum LoadChangesStage {
    L1BatchHeader,
    ProtectiveReads,
    TouchedSlots,
    InitialWritesForZeroValues,
}

impl LoadChangesStage {
    const COUNT_HISTOGRAM_NAME: &'static str = "server.metadata_calculator.load_changes.count";
}

impl ReportStage for LoadChangesStage {
    const HISTOGRAM_NAME: &'static str = "server.metadata_calculator.load_changes.latency";

    fn as_tag(self) -> &'static str {
        match self {
            Self::L1BatchHeader => "load_l1_batch_header",
            Self::ProtectiveReads => "load_protective_reads",
            Self::TouchedSlots => "load_touched_slots",
            Self::InitialWritesForZeroValues => "load_initial_writes_for_zero_values",
        }
    }
}

/// Latency metric for a certain stage of the tree update.
#[derive(Debug)]
#[must_use = "Tree latency should be `report`ed"]
pub(super) struct UpdateTreeLatency<S> {
    stage: S,
    start: Instant,
}

impl<S: ReportStage> UpdateTreeLatency<S> {
    pub fn report(self) {
        self.report_inner(None);
    }

    fn report_inner(self, record_count: Option<usize>) {
        let elapsed = self.start.elapsed();
        let stage = self.stage.as_tag();
        metrics::histogram!(S::HISTOGRAM_NAME, elapsed, "stage" => stage);

        if let Some(record_count) = record_count {
            vlog::debug!(
                "Metadata calculator stage `{stage}` with {record_count} records completed in {elapsed:?}"
            );
        } else {
            vlog::debug!("Metadata calculator stage `{stage}` completed in {elapsed:?}");
        }
    }
}

impl UpdateTreeLatency<LoadChangesStage> {
    pub fn report_with_count(self, count: usize) {
        let stage = self.stage.as_tag();
        self.report_inner(Some(count));
        metrics::histogram!(LoadChangesStage::COUNT_HISTOGRAM_NAME, count as f64, "stage" => stage);
    }
}

impl MetadataCalculator {
    pub(super) fn update_metrics(
        mode: MerkleTreeMode,
        batch_headers: &[L1BatchHeader],
        total_logs: usize,
        start: Instant,
    ) {
        let mode_tag = match mode {
            MerkleTreeMode::Full => "full",
            MerkleTreeMode::Lightweight => "lightweight",
        };

        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency",
            start.elapsed()
        );
        if total_logs > 0 {
            metrics::histogram!(
                "server.metadata_calculator.update_tree.per_log.latency",
                start.elapsed().div_f32(total_logs as f32)
            );
        }

        let total_tx: usize = batch_headers.iter().map(L1BatchHeader::tx_count).sum();
        let total_l1_tx_count: u64 = batch_headers
            .iter()
            .map(|batch| u64::from(batch.l1_tx_count))
            .sum();
        metrics::counter!("server.processed_txs", total_tx as u64, "stage" => "tree");
        metrics::counter!("server.processed_l1_txs", total_l1_tx_count, "stage" => "tree");
        metrics::histogram!("server.metadata_calculator.log_batch", total_logs as f64);
        metrics::histogram!(
            "server.metadata_calculator.blocks_batch",
            batch_headers.len() as f64
        );

        let first_batch_number = batch_headers.first().unwrap().number.0;
        let last_batch_number = batch_headers.last().unwrap().number.0;
        vlog::info!(
            "L1 batches #{:?} processed in tree",
            first_batch_number..=last_batch_number
        );
        metrics::gauge!(
            "server.block_number",
            last_batch_number as f64,
            "stage" => format!("tree_{mode_tag}_mode")
        );

        let latency =
            seconds_since_epoch().saturating_sub(batch_headers.first().unwrap().timestamp);
        metrics::histogram!(
            "server.block_latency",
            latency as f64,
            "stage" => format!("tree_{mode_tag}_mode")
        );
    }
}
