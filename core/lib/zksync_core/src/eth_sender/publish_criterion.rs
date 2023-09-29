use async_trait::async_trait;
use chrono::Utc;

use std::fmt;

use zksync_dal::StorageProcessor;
use zksync_types::{
    aggregated_operations::AggregatedActionType, commitment::L1BatchWithMetadata, L1BatchNumber,
};

use super::metrics::METRICS;
use crate::gas_tracker::agg_l1_batch_base_cost;

#[async_trait]
pub trait L1BatchPublishCriterion: fmt::Debug + Send + Sync {
    // Takes `&self` receiver for the trait to be object-safe
    fn name(&self) -> &'static str;

    /// Returns `None` if there is no need to publish any L1 batches.
    /// Otherwise, returns the number of the last L1 batch that needs to be published.
    async fn last_l1_batch_to_publish(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        consecutive_l1_batches: &[L1BatchWithMetadata],
        last_sealed_l1_batch: L1BatchNumber,
    ) -> Option<L1BatchNumber>;
}

#[derive(Debug)]
pub struct NumberCriterion {
    pub op: AggregatedActionType,
    /// Maximum number of L1 batches to be packed together.
    pub limit: u32,
}

#[async_trait]
impl L1BatchPublishCriterion for NumberCriterion {
    fn name(&self) -> &'static str {
        "l1_batch_number"
    }

    async fn last_l1_batch_to_publish(
        &mut self,
        _storage: &mut StorageProcessor<'_>,
        consecutive_l1_batches: &[L1BatchWithMetadata],
        _last_sealed_l1_batch: L1BatchNumber,
    ) -> Option<L1BatchNumber> {
        let mut batch_numbers = consecutive_l1_batches
            .iter()
            .map(|batch| batch.header.number.0);

        let first = batch_numbers.next()?;
        let last_batch_number = batch_numbers.last().unwrap_or(first);
        let batch_count = last_batch_number - first + 1;
        if batch_count >= self.limit {
            let result = L1BatchNumber(first + self.limit - 1);
            tracing::debug!(
                "`l1_batch_number` publish criterion (limit={}) triggered for op {} with L1 batch range {:?}",
                self.limit,
                self.op,
                first..=result.0
            );
            METRICS.block_aggregation_reason[&(self.op, "number").into()].inc();
            Some(result)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct TimestampDeadlineCriterion {
    pub op: AggregatedActionType,
    /// Maximum L1 batch age in seconds. Once reached, we pack and publish all the available L1 batches.
    pub deadline_seconds: u64,
    /// If `max_allowed_lag` is `Some(_)` and last batch sent to L1 is more than `max_allowed_lag` behind,
    /// it means that sender is lagging significantly and we shouldn't apply this criteria to use all capacity
    /// and avoid packing small ranges.
    pub max_allowed_lag: Option<usize>,
}

#[async_trait]
impl L1BatchPublishCriterion for TimestampDeadlineCriterion {
    fn name(&self) -> &'static str {
        "timestamp"
    }

    async fn last_l1_batch_to_publish(
        &mut self,
        _storage: &mut StorageProcessor<'_>,
        consecutive_l1_batches: &[L1BatchWithMetadata],
        last_sealed_l1_batch: L1BatchNumber,
    ) -> Option<L1BatchNumber> {
        let first_l1_batch = consecutive_l1_batches.iter().next()?;
        let last_l1_batch_number = consecutive_l1_batches.iter().last()?.header.number.0;
        if let Some(max_allowed_lag) = self.max_allowed_lag {
            if last_sealed_l1_batch.0 - last_l1_batch_number >= max_allowed_lag as u32 {
                return None;
            }
        }
        let oldest_l1_batch_age_seconds =
            Utc::now().timestamp() as u64 - first_l1_batch.header.timestamp;
        if oldest_l1_batch_age_seconds >= self.deadline_seconds {
            let result = consecutive_l1_batches
                .last()
                .unwrap_or(first_l1_batch)
                .header
                .number;
            tracing::debug!(
                "`timestamp` publish criterion triggered for op {} with L1 batch range {:?}",
                self.op,
                first_l1_batch.header.number.0..=result.0
            );
            METRICS.block_aggregation_reason[&(self.op, "timestamp").into()].inc();
            Some(result)
        } else {
            None
        }
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
        &self,
        storage: &mut StorageProcessor<'_>,
        batch_number: L1BatchNumber,
    ) -> u32 {
        storage
            .blocks_dal()
            .get_l1_batches_predicted_gas(batch_number..=batch_number, self.op)
            .await
            .unwrap()
    }
}

#[async_trait]
impl L1BatchPublishCriterion for GasCriterion {
    fn name(&self) -> &'static str {
        "gas_limit"
    }

    async fn last_l1_batch_to_publish(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        consecutive_l1_batches: &[L1BatchWithMetadata],
        _last_sealed_l1_batch: L1BatchNumber,
    ) -> Option<L1BatchNumber> {
        let base_cost = agg_l1_batch_base_cost(self.op);
        assert!(
            self.gas_limit > base_cost,
            "Config max gas cost for operations is too low"
        );
        // We're not sure our predictions are accurate, so it's safer to lower the gas limit by 10%
        let mut gas_left = (self.gas_limit as f64 * 0.9).round() as u32 - base_cost;

        let mut last_l1_batch = None;
        for (index, l1_batch) in consecutive_l1_batches.iter().enumerate() {
            let batch_gas = self.get_gas_amount(storage, l1_batch.header.number).await;
            if batch_gas >= gas_left {
                if index == 0 {
                    panic!(
                        "L1 batch #{} requires {} gas, which is more than the range limit of {}",
                        l1_batch.header.number, batch_gas, self.gas_limit
                    );
                }
                last_l1_batch = Some(L1BatchNumber(l1_batch.header.number.0 - 1));
                break;
            } else {
                gas_left -= batch_gas;
            }
        }

        if let Some(last_l1_batch) = last_l1_batch {
            let first_l1_batch_number = consecutive_l1_batches.first().unwrap().header.number.0;
            tracing::debug!(
                "`gas_limit` publish criterion (gas={}) triggered for op {} with L1 batch range {:?}",
                self.gas_limit - gas_left,
                self.op,
                first_l1_batch_number..=last_l1_batch.0
            );
            METRICS.block_aggregation_reason[&(self.op, "gas").into()].inc();
        }
        last_l1_batch
    }
}

#[derive(Debug)]
pub struct DataSizeCriterion {
    pub op: AggregatedActionType,
    pub data_limit: usize,
}

#[async_trait]
impl L1BatchPublishCriterion for DataSizeCriterion {
    fn name(&self) -> &'static str {
        "data_size"
    }

    async fn last_l1_batch_to_publish(
        &mut self,
        _storage: &mut StorageProcessor<'_>,
        consecutive_l1_batches: &[L1BatchWithMetadata],
        _last_sealed_l1_batch: L1BatchNumber,
    ) -> Option<L1BatchNumber> {
        const STORED_BLOCK_INFO_SIZE: usize = 96; // size of `StoredBlockInfo` solidity struct
        let mut data_size_left = self.data_limit - STORED_BLOCK_INFO_SIZE;

        for (index, l1_batch) in consecutive_l1_batches.iter().enumerate() {
            if data_size_left < l1_batch.l1_commit_data_size() {
                if index == 0 {
                    panic!(
                        "L1 batch #{} requires {} data, which is more than the range limit of {}",
                        l1_batch.header.number,
                        l1_batch.l1_commit_data_size(),
                        self.data_limit
                    );
                }

                let first_l1_batch_number = consecutive_l1_batches.first().unwrap().header.number.0;
                let output = l1_batch.header.number - 1;
                tracing::debug!(
                    "`data_size` publish criterion (data={}) triggered for op {} with L1 batch range {:?}",
                    self.data_limit - data_size_left,
                    self.op,
                    first_l1_batch_number..=output.0
                );
                METRICS.block_aggregation_reason[&(self.op, "data_size").into()].inc();
                return Some(output);
            }
            data_size_left -= l1_batch.l1_commit_data_size();
        }

        None
    }
}
