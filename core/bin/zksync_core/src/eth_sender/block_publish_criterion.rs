use crate::gas_tracker::agg_block_base_cost;
use async_trait::async_trait;
use chrono::Utc;
use zksync_dal::StorageProcessor;
use zksync_types::commitment::BlockWithMetadata;
use zksync_types::{aggregated_operations::AggregatedActionType, L1BatchNumber};

#[async_trait]
pub trait BlockPublishCriterion: std::fmt::Debug + Send + Sync {
    // returns None if there is no need to publish any blocks
    // otherwise returns the block height of the last block that needs to be published
    async fn last_block_to_publish(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        consecutive_blocks: &[BlockWithMetadata],
        last_sealed_block: L1BatchNumber,
    ) -> Option<L1BatchNumber>;

    fn name(&self) -> &'static str;
}

#[derive(Debug)]
pub struct BlockNumberCriterion {
    pub op: AggregatedActionType,
    // maximum number of blocks to be packed together
    pub limit: u32,
}

#[async_trait]
impl BlockPublishCriterion for BlockNumberCriterion {
    async fn last_block_to_publish(
        &mut self,
        _storage: &mut StorageProcessor<'_>,
        consecutive_blocks: &[BlockWithMetadata],
        _last_sealed_block: L1BatchNumber,
    ) -> Option<L1BatchNumber> {
        {
            let mut block_heights = consecutive_blocks.iter().map(|block| block.header.number.0);
            block_heights.next().and_then(|first| {
                let last_block_height = block_heights.last().unwrap_or(first);
                let blocks_count = last_block_height - first + 1;
                if blocks_count >= self.limit {
                    let result = L1BatchNumber(first + self.limit - 1);
                    vlog::debug!(
                        "{} block range {}-{}: NUMBER {} triggered",
                        self.op.to_string(),
                        first,
                        result.0,
                        self.limit
                    );
                    metrics::counter!(
                        "server.eth_sender.block_aggregation_reason",
                        1,
                        "type" => "number",
                        "op" => self.op.to_string()
                    );
                    Some(result)
                } else {
                    None
                }
            })
        }
    }

    fn name(&self) -> &'static str {
        "block_number"
    }
}

#[derive(Debug)]
pub struct TimestampDeadlineCriterion {
    pub op: AggregatedActionType,
    // Maximum block age in seconds. Once reached, we pack and publish all the available blocks.
    pub deadline_seconds: u64,
    // If `max_allowed_lag` is some and last block sent to L1 is more than `max_allowed_lag` behind,
    // it means that sender is lagging significantly and we shouldn't apply this criteria to use all capacity
    // and avoid packing small ranges.
    pub max_allowed_lag: Option<usize>,
}

#[async_trait]
impl BlockPublishCriterion for TimestampDeadlineCriterion {
    async fn last_block_to_publish(
        &mut self,
        _storage: &mut StorageProcessor<'_>,
        consecutive_blocks: &[BlockWithMetadata],
        last_sealed_block: L1BatchNumber,
    ) -> Option<L1BatchNumber> {
        consecutive_blocks.iter().next().and_then(|first_block| {
            let last_block_number = consecutive_blocks.iter().last().unwrap().header.number.0;
            if let Some(max_allowed_lag) = self.max_allowed_lag {
                if last_sealed_block.0 - last_block_number >= max_allowed_lag as u32 {
                    return None;
                }
            }
            let oldest_block_age_seconds =
                Utc::now().timestamp() as u64 - first_block.header.timestamp;
            if oldest_block_age_seconds >= self.deadline_seconds {
                let result = consecutive_blocks
                    .last()
                    .unwrap_or(first_block)
                    .header
                    .number;
                vlog::debug!(
                    "{} block range {}-{}: TIMESTAMP triggered",
                    self.op.to_string(),
                    first_block.header.number.0,
                    result.0
                );
                metrics::counter!(
                    "server.eth_sender.block_aggregation_reason",
                    1,
                    "type" => "timestamp",
                    "op" => self.op.to_string()
                );
                Some(result)
            } else {
                None
            }
        })
    }
    fn name(&self) -> &'static str {
        "timestamp"
    }
}

#[derive(Debug)]
pub struct GasCriterion {
    pub op: AggregatedActionType,
    pub gas_limit: u32,
}

impl GasCriterion {
    pub fn new(op: AggregatedActionType, gas_limit: u32) -> GasCriterion {
        GasCriterion { op, gas_limit }
    }

    async fn get_gas_amount(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        block_number: L1BatchNumber,
    ) -> u32 {
        storage
            .blocks_dal()
            .get_blocks_predicted_gas(block_number, block_number, self.op)
            .await
    }
}

#[async_trait]
impl BlockPublishCriterion for GasCriterion {
    async fn last_block_to_publish(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        consecutive_blocks: &[BlockWithMetadata],
        _last_sealed_block: L1BatchNumber,
    ) -> Option<L1BatchNumber> {
        let base_cost = agg_block_base_cost(self.op);
        assert!(
            self.gas_limit > base_cost,
            "Config max gas cost for operations is too low"
        );
        // We're not sure our predictions are accurate, so it's safer to lower the gas limit by 10%
        let mut gas_left = (self.gas_limit as f64 * 0.9).round() as u32 - base_cost;

        let mut last_block: Option<L1BatchNumber> = None;
        for (index, block) in consecutive_blocks.iter().enumerate() {
            let block_gas = self.get_gas_amount(storage, block.header.number).await;
            if block_gas >= gas_left {
                if index == 0 {
                    panic!(
                        "block {} requires {} gas, which is more than the range limit of {}",
                        block.header.number, block_gas, self.gas_limit
                    )
                }
                last_block = Some(L1BatchNumber(block.header.number.0 - 1));
                break;
            } else {
                gas_left -= block_gas;
            }
        }

        if last_block.is_some() {
            vlog::debug!(
                "{} block range {}-{}: GAS {} triggered",
                self.op.to_string(),
                consecutive_blocks.first().unwrap().header.number.0,
                last_block.unwrap().0,
                self.gas_limit - gas_left,
            );
            metrics::counter!(
                "server.eth_sender.block_aggregation_reason",
                1,
                "type" => "gas",
                "op" => self.op.to_string()
            );
        }
        last_block
    }
    fn name(&self) -> &'static str {
        "gas_limit"
    }
}

#[derive(Debug)]
pub struct DataSizeCriterion {
    pub op: AggregatedActionType,
    pub data_limit: usize,
}

#[async_trait]
impl BlockPublishCriterion for DataSizeCriterion {
    async fn last_block_to_publish(
        &mut self,
        _storage: &mut StorageProcessor<'_>,
        consecutive_blocks: &[BlockWithMetadata],
        _last_sealed_block: L1BatchNumber,
    ) -> Option<L1BatchNumber> {
        const STORED_BLOCK_INFO_SIZE: usize = 96; // size of `StoredBlockInfo` solidity struct
        let mut data_size_left = self.data_limit - STORED_BLOCK_INFO_SIZE;

        for (index, block) in consecutive_blocks.iter().enumerate() {
            if data_size_left < block.l1_commit_data_size() {
                if index == 0 {
                    panic!(
                        "block {} requires {} data, which is more than the range limit of {}",
                        block.header.number,
                        block.l1_commit_data_size(),
                        self.data_limit
                    )
                }
                vlog::debug!(
                    "{} block range {}-{}: DATA LIMIT {} triggered",
                    self.op.to_string(),
                    consecutive_blocks.first().unwrap().header.number.0,
                    block.header.number.0 - 1,
                    self.data_limit - data_size_left,
                );
                metrics::counter!(
                    "server.eth_sender.block_aggregation_reason",
                    1,
                    "type" => "data_size",
                    "op" => self.op.to_string()
                );
                return Some(block.header.number - 1);
            }
            data_size_left -= block.l1_commit_data_size();
        }

        None
    }

    fn name(&self) -> &'static str {
        "data_size"
    }
}
