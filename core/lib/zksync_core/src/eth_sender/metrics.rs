//! Metrics for the Ethereum sender component.

use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics};

use std::{fmt, time::Duration};

use zksync_dal::StorageProcessor;
use zksync_types::{aggregated_operations::AggregatedActionType, eth_sender::EthTx};
use zksync_utils::time::seconds_since_epoch;

use crate::metrics::{BlockL1Stage, BlockStage, APP_METRICS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "kind", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub(super) enum PubdataKind {
    L2ToL1MessagesCompressed,
    InitialWritesCompressed,
    RepeatedWritesCompressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "type")]
pub(super) struct ActionTypeLabel(AggregatedActionType);

impl From<AggregatedActionType> for ActionTypeLabel {
    fn from(action_type: AggregatedActionType) -> Self {
        Self(action_type)
    }
}

impl fmt::Display for ActionTypeLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct AggregationReasonLabels {
    r#type: &'static str,
    op: ActionTypeLabel,
}

impl From<(AggregatedActionType, &'static str)> for AggregationReasonLabels {
    fn from((op, r#type): (AggregatedActionType, &'static str)) -> Self {
        Self {
            r#type,
            op: op.into(),
        }
    }
}

/// Roughly exponential buckets for fees (100M – 500B).
const FEE_BUCKETS: Buckets = Buckets::values(&[
    1e7, 2e7, 5e7, 1e8, 2e8, 5e8, 1e9, 2e9, 5e9, 1e10, 2e10, 5e10, 1e11, 2e11, 5e11,
]);
/// Roughly exponential buckets for gas (10k – 50M).
const GAS_BUCKETS: Buckets =
    Buckets::values(&[1e4, 2e4, 5e4, 1e5, 2e5, 5e5, 1e6, 2e6, 5e6, 1e7, 2e7, 5e7]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_eth_sender")]
pub(super) struct EthSenderMetrics {
    /// Latency of collecting Ethereum sender metrics.
    #[metrics(buckets = Buckets::LATENCIES)]
    metrics_latency: Histogram<Duration>,
    /// Size of data published on L1 for an L1 batch.
    #[metrics(buckets = Buckets::exponential(16.0..=4_096.0, 2.0))]
    pub pubdata_size: Family<PubdataKind, Histogram<usize>>,
    /// Size of the L1 batch range for a certain Ethereum sender operation.
    #[metrics(buckets = Buckets::linear(1.0..=10.0, 1.0))]
    pub block_range_size: Family<ActionTypeLabel, Histogram<u64>>,
    /// Number of transactions resent by the Ethereum sender.
    pub transaction_resent: Counter,
    #[metrics(buckets = FEE_BUCKETS)]
    pub used_base_fee_per_gas: Histogram<u64>,
    #[metrics(buckets = FEE_BUCKETS)]
    pub used_priority_fee_per_gas: Histogram<u64>,
    /// Last L1 block observed by the Ethereum sender.
    pub last_known_l1_block: Gauge<u64>,
    /// Number of inflight txs produced by the Ethereum sender.
    pub number_of_inflight_txs: Gauge<usize>,
    #[metrics(buckets = GAS_BUCKETS)]
    pub l1_gas_used: Family<ActionTypeLabel, Histogram<f64>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub l1_tx_mined_latency: Family<ActionTypeLabel, Histogram<Duration>>,
    #[metrics(buckets = &[1.0, 2.0, 3.0, 5.0, 7.0, 10.0, 20.0, 30.0, 50.0])]
    pub l1_blocks_waited_in_mempool: Family<ActionTypeLabel, Histogram<u64>>,
    /// Number of L1 batches aggregated for publishing with a specific reason.
    pub block_aggregation_reason: Family<AggregationReasonLabels, Counter>,
}

impl EthSenderMetrics {
    pub async fn track_eth_tx_metrics(
        &self,
        connection: &mut StorageProcessor<'_>,
        l1_stage: BlockL1Stage,
        tx: &EthTx,
    ) {
        let metrics_latency = self.metrics_latency.start();
        let stage = BlockStage::L1 {
            l1_stage,
            tx_type: tx.tx_type,
        };

        let l1_batch_headers = connection
            .blocks_dal()
            .get_l1_batches_for_eth_tx_id(tx.id)
            .await
            .unwrap();

        // This should be only the case when some blocks were reverted.
        if l1_batch_headers.is_empty() {
            tracing::warn!("No L1 batches were found for eth_tx with id = {}", tx.id);
            return;
        }

        for header in l1_batch_headers {
            APP_METRICS.block_latency[&stage].observe(Duration::from_secs(
                seconds_since_epoch() - header.timestamp,
            ));
            APP_METRICS.processed_txs[&stage.into()].inc_by(header.tx_count() as u64);
            APP_METRICS.processed_l1_txs[&stage.into()].inc_by(header.tx_count() as u64);
        }
        metrics_latency.observe();
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<EthSenderMetrics> = vise::Global::new();
