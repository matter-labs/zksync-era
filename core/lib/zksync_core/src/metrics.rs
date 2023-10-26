//! Application-wide metrics.

use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics};

use std::{fmt, time::Duration};

use zksync_dal::transactions_dal::L2TxSubmissionResult;
use zksync_types::{aggregated_operations::AggregatedActionType, proofs::AggregationRound};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage")]
pub(crate) enum InitStage {
    HttpApi,
    WsApi,
    ContractVerificationApi,
    StateKeeper,
    EthWatcher,
    EthTxAggregator,
    EthTxManager,
    DataFetcher,
    Tree,
    WitnessGenerator(AggregationRound),
    BasicWitnessInputProducer,
}

impl fmt::Display for InitStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HttpApi => formatter.write_str("http_api"),
            Self::WsApi => formatter.write_str("ws_api"),
            Self::ContractVerificationApi => formatter.write_str("contract_verification_api"),
            Self::StateKeeper => formatter.write_str("state_keeper"),
            Self::EthWatcher => formatter.write_str("eth_watcher"),
            Self::EthTxAggregator => formatter.write_str("eth_tx_aggregator"),
            Self::EthTxManager => formatter.write_str("eth_tx_manager"),
            Self::DataFetcher => formatter.write_str("data_fetchers"),
            Self::Tree => formatter.write_str("tree"),
            Self::WitnessGenerator(round) => write!(formatter, "witness_generator_{round:?}"),
            Self::BasicWitnessInputProducer => formatter.write_str("basic_witness_input_producer"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BlockL1Stage {
    Saved,
    Mined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub(crate) enum L1StageLatencyLabel {
    UncommittedBlock,
    UnprovedBlock,
    UnexecutedBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage")]
pub(crate) enum BlockStage {
    Sealed,
    Tree,
    MetadataCalculated,
    MerkleProofCalculated,
    L1 {
        l1_stage: BlockL1Stage,
        tx_type: AggregatedActionType,
    },
}

impl fmt::Display for BlockStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sealed => formatter.write_str("sealed"),
            Self::Tree => formatter.write_str("tree"),
            Self::MetadataCalculated => formatter.write_str("metadata_calculated"),
            Self::MerkleProofCalculated => formatter.write_str("merkle_proof_calculated"),
            Self::L1 { l1_stage, tx_type } => {
                let l1_stage = match l1_stage {
                    BlockL1Stage::Saved => "save", // not "saved" for backward compatibility
                    BlockL1Stage::Mined => "mined",
                };
                write!(formatter, "l1_{l1_stage}_{tx_type}")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage")]
pub(crate) enum TxStage {
    Mempool(L2TxSubmissionResult),
    Proxied,
    StateKeeper,
    Block(BlockStage),
}

impl fmt::Display for TxStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mempool(L2TxSubmissionResult::Duplicate) => formatter.write_str("duplicated"),
            Self::Mempool(L2TxSubmissionResult::AlreadyExecuted) => {
                formatter.write_str("already_executed")
            }
            Self::Mempool(result) => write!(formatter, "mempool_{result}"),
            Self::Proxied => formatter.write_str("proxied"),
            Self::StateKeeper => formatter.write_str("state_keeper"),
            Self::Block(stage) => fmt::Display::fmt(stage, formatter),
        }
    }
}

impl From<BlockStage> for TxStage {
    fn from(stage: BlockStage) -> Self {
        Self::Block(stage)
    }
}

impl TxStage {
    pub const fn added_to_mempool() -> Self {
        Self::Mempool(L2TxSubmissionResult::Added)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum MiniblockStage {
    Sealed,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server")]
pub(crate) struct AppMetrics {
    /// Latency to initialize a specific server component.
    pub init_latency: Family<InitStage, Gauge<Duration>>,
    pub block_number: Family<BlockStage, Gauge<u64>>,
    pub miniblock_number: Family<MiniblockStage, Gauge<u64>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub block_latency: Family<BlockStage, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub miniblock_latency: Family<MiniblockStage, Histogram<Duration>>,
    /// Latest virtual block number produced by the state keeper.
    pub miniblock_virtual_block_number: Family<MiniblockStage, Gauge<u64>>,
    pub processed_txs: Family<TxStage, Counter>,
    pub processed_l1_txs: Family<TxStage, Counter>,

    pub blocks_state_block_eth_stage_latency: Family<L1StageLatencyLabel, Gauge<u64>>,
}

#[vise::register]
pub(crate) static APP_METRICS: vise::Global<AppMetrics> = vise::Global::new();

/// Type of VM interaction with the storage. Used both for API server and state keeper metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "interaction", rename_all = "snake_case")]
pub(crate) enum InteractionType {
    Missed,
    GetValue,
    SetValue,
    Total,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "component", rename_all = "snake_case")]
pub(crate) enum CheckerComponent {
    ConsistencyChecker,
    ReorgDetector,
}

/// General-purpose external node metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "external_node")]
pub(crate) struct ExternalNodeMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub update_batch_statuses: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub batch_status_updater_loop_iteration: Histogram<Duration>,
    /// Is the external node currently synced?
    pub synced: Gauge<u64>,
    /// Current sync lag of the external node.
    pub sync_lag: Gauge<u64>,
    /// Number of the last L1 batch checked by the reorg detector or consistency checker.
    pub last_correct_batch: Family<CheckerComponent, Gauge<u64>>,
    /// Number of the last miniblock checked by the reorg detector or consistency checker.
    pub last_correct_miniblock: Family<CheckerComponent, Gauge<u64>>,
}

#[vise::register]
pub(crate) static EN_METRICS: vise::Global<ExternalNodeMetrics> = vise::Global::new();
