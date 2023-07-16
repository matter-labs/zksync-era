//! Metrics for `MetadataCalculator`.

use std::time::Instant;

use zksync_types::block::L1BatchHeader;
use zksync_utils::time::seconds_since_epoch;

use super::{MetadataCalculator, MetadataCalculatorMode};

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

impl TreeUpdateStage {
    pub fn as_str(self) -> &'static str {
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

    pub fn start(self) -> UpdateTreeLatency {
        UpdateTreeLatency {
            stage: self,
            start: Instant::now(),
        }
    }
}

/// Latency metric for a certain stage of the tree update.
#[derive(Debug)]
#[must_use = "Tree latency should be `report`ed"]
pub(super) struct UpdateTreeLatency {
    stage: TreeUpdateStage,
    start: Instant,
}

impl UpdateTreeLatency {
    pub fn report(self) {
        let elapsed = self.start.elapsed();
        metrics::histogram!(
            "server.metadata_calculator.update_tree.latency.stage",
            elapsed,
            "stage" => self.stage.as_str()
        );
        vlog::trace!(
            "Metadata calculator stage `{stage}` completed in {elapsed:?}",
            stage = self.stage.as_str()
        );
    }
}

impl MetadataCalculator {
    pub(super) fn update_metrics(
        mode: MetadataCalculatorMode,
        block_headers: &[L1BatchHeader],
        total_logs: usize,
        start: Instant,
    ) {
        let mode_tag = mode.as_tag();

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

        let total_tx: usize = block_headers.iter().map(|block| block.tx_count()).sum();
        let total_l1_tx: u64 = block_headers
            .iter()
            .map(|block| u64::from(block.l1_tx_count))
            .sum();
        metrics::counter!("server.processed_txs", total_tx as u64, "stage" => "tree");
        metrics::counter!("server.processed_l1_txs", total_l1_tx, "stage" => "tree");
        metrics::histogram!("server.metadata_calculator.log_batch", total_logs as f64);
        metrics::histogram!(
            "server.metadata_calculator.blocks_batch",
            block_headers.len() as f64
        );

        let first_block_number = block_headers.first().unwrap().number.0;
        let last_block_number = block_headers.last().unwrap().number.0;
        vlog::info!(
            "L1 batches #{:?} processed in tree",
            first_block_number..=last_block_number
        );
        metrics::gauge!(
            "server.block_number",
            last_block_number as f64,
            "stage" => format!("tree_{mode_tag}_mode")
        );

        let latency =
            seconds_since_epoch().saturating_sub(block_headers.first().unwrap().timestamp);
        metrics::histogram!(
            "server.block_latency",
            latency as f64,
            "stage" => format!("tree_{mode_tag}_mode")
        );
    }
}
