//! Metrics for the Ethereum sender component.

use std::{
    fmt,
    time::{Duration, SystemTime},
};

use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_shared_metrics::{BlockL1Stage, BlockStage, L1Stage, L2BlockStage, APP_METRICS};
use zksync_types::{
    aggregated_operations::{
        AggregatedActionType, L1BatchAggregatedActionType, L2BlockAggregatedActionType,
    },
    eth_sender::{EthTx, L1BlockNumbers},
};

use crate::abstract_l1_interface::OperatorType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "kind", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub(super) enum PubdataKind {
    UserL2ToL1Logs,
    StateDiffs,
    LongL2ToL1Messages,
    RawPublishedBytecodes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "block_number_variant", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub(super) enum BlockNumberVariant {
    Latest,
    Finalized,
    Safe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "type")]
pub(super) struct ActionTypeLabel(AggregatedActionType);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "type")]
pub(super) struct L1BatchActionTypeLabel(L1BatchAggregatedActionType);

impl From<L1BatchAggregatedActionType> for L1BatchActionTypeLabel {
    fn from(action_type: L1BatchAggregatedActionType) -> Self {
        Self(action_type)
    }
}

impl fmt::Display for L1BatchActionTypeLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "type")]
pub(super) struct L2BlockActionTypeLabel(L2BlockAggregatedActionType);

impl From<L2BlockAggregatedActionType> for L2BlockActionTypeLabel {
    fn from(action_type: L2BlockAggregatedActionType) -> Self {
        Self(action_type)
    }
}

impl fmt::Display for L2BlockActionTypeLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "transaction_type", rename_all = "snake_case")]
pub(super) enum TransactionType {
    Blob,
    Regular,
}

impl From<AggregatedActionType> for ActionTypeLabel {
    fn from(action_type: AggregatedActionType) -> Self {
        Self(action_type)
    }
}

impl From<L1BatchAggregatedActionType> for ActionTypeLabel {
    fn from(action_type: L1BatchAggregatedActionType) -> Self {
        Self(AggregatedActionType::L1Batch(action_type))
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

impl From<(L1BatchAggregatedActionType, &'static str)> for AggregationReasonLabels {
    fn from((op, r#type): (L1BatchAggregatedActionType, &'static str)) -> Self {
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
    #[metrics(buckets = Buckets::exponential(1024.0..=131_072.0, 2.0))]
    pub pubdata_size: Family<PubdataKind, Histogram<usize>>,
    /// Size of the L1 batch range for a certain Ethereum sender operation.
    #[metrics(buckets = Buckets::linear(1.0..=10.0, 1.0))]
    pub block_range_size: Family<L1BatchActionTypeLabel, Histogram<u64>>,
    /// Size of the L2 blocks range for a certain Ethereum sender operation.
    #[metrics(buckets = Buckets::linear(1.0..=10.0, 1.0))]
    pub l2_blocks_range_size: Family<L2BlockActionTypeLabel, Histogram<u64>>,
    /// Number of transactions resent by the Ethereum sender.
    pub transaction_resent: Counter,
    #[metrics(buckets = FEE_BUCKETS)]
    pub used_base_fee_per_gas: Family<TransactionType, Histogram<u64>>,
    #[metrics(buckets = FEE_BUCKETS)]
    pub used_priority_fee_per_gas: Family<TransactionType, Histogram<u64>>,
    #[metrics(buckets = FEE_BUCKETS)]
    pub used_blob_fee_per_gas: Family<TransactionType, Histogram<u64>>,
    /// Last L1 block observed by the Ethereum sender.
    pub last_known_l1_block: Family<BlockNumberVariant, Gauge<usize>>,
    /// Number of in-flight txs produced by the Ethereum sender.
    pub number_of_inflight_txs: Family<OperatorType, Gauge<usize>>,
    #[metrics(buckets = GAS_BUCKETS)]
    pub l1_gas_used: Family<ActionTypeLabel, Histogram<f64>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub l1_tx_fast_finalized_latency: Family<ActionTypeLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub l1_tx_mined_latency: Family<ActionTypeLabel, Histogram<Duration>>,
    #[metrics(buckets = & [1.0, 2.0, 3.0, 5.0, 7.0, 10.0, 20.0, 30.0, 50.0])]
    pub l1_blocks_waited_in_mempool: Family<ActionTypeLabel, Histogram<u64>>,
    /// Number of L1 batches aggregated for publishing with a specific reason.
    pub block_aggregation_reason: Family<AggregationReasonLabels, Counter>,
    pub l1_transient_errors: Counter,
}

impl EthSenderMetrics {
    pub fn track_block_numbers(&self, l1_block_numbers: &L1BlockNumbers) {
        self.last_known_l1_block[&BlockNumberVariant::Latest]
            .set(l1_block_numbers.latest.0 as usize);
        self.last_known_l1_block[&BlockNumberVariant::Finalized]
            .set(l1_block_numbers.finalized.0 as usize);
        self.last_known_l1_block[&BlockNumberVariant::Safe]
            .set(l1_block_numbers.fast_finality.0 as usize);
    }

    pub async fn track_eth_tx_metrics(
        &self,
        connection: &mut Connection<'_, Core>,
        l1_stage: L1Stage,
        tx: &EthTx,
    ) {
        match tx.tx_type {
            AggregatedActionType::L2Block(action_type) => {
                let metrics_latency = self.metrics_latency.start();
                let stage = L2BlockStage::L1 {
                    l1_stage,
                    tx_type: action_type,
                };

                let l2_blocks_statistics = connection
                    .blocks_dal()
                    .get_l2_blocks_statistics_for_eth_tx_id(tx.id)
                    .await
                    .unwrap();

                // This should be only the case when some blocks were reverted.
                if l2_blocks_statistics.is_empty() {
                    tracing::warn!("No L2 blocks were found for eth_tx with id = {}", tx.id);
                    return;
                }

                let duration_since_epoch = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("incorrect system time");
                for statistics in l2_blocks_statistics {
                    let block_latency = duration_since_epoch
                        .saturating_sub(Duration::from_secs(statistics.timestamp));
                    APP_METRICS.miniblock_latency[&stage].observe(block_latency);
                    APP_METRICS.processed_txs[&stage.into()]
                        .inc_by(statistics.l2_tx_count as u64 + statistics.l1_tx_count as u64);
                }
                metrics_latency.observe();
            }
            AggregatedActionType::L1Batch(action_type) => {
                let metrics_latency = self.metrics_latency.start();
                let stage = BlockStage::L1 {
                    l1_stage,
                    tx_type: action_type,
                };

                let l1_batches_statistics = connection
                    .blocks_dal()
                    .get_l1_batches_statistics_for_eth_tx_id(tx.id)
                    .await
                    .unwrap();

                // This should be only the case when some blocks were reverted.
                if l1_batches_statistics.is_empty() {
                    tracing::warn!("No L1 batches were found for eth_tx with id = {}", tx.id);
                    return;
                }

                let duration_since_epoch = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("incorrect system time");
                for statistics in l1_batches_statistics {
                    let block_latency = duration_since_epoch
                        .saturating_sub(Duration::from_secs(statistics.timestamp));
                    APP_METRICS.block_latency[&stage].observe(block_latency);
                    APP_METRICS.processed_txs[&stage.into()]
                        .inc_by(statistics.l2_tx_count as u64 + statistics.l1_tx_count as u64);
                    APP_METRICS.processed_l1_txs[&stage.into()]
                        .inc_by(statistics.l1_tx_count as u64);
                }
                metrics_latency.observe();
            }
        }
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<EthSenderMetrics> = vise::Global::new();
