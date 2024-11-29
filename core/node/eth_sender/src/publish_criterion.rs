use std::{fmt, ops};

use async_trait::async_trait;
use chrono::Utc;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::{
    aggregated_operations::{
        AggregatedActionType, L1_BATCH_EXECUTE_BASE_COST, L1_OPERATION_EXECUTE_COST,
    },
    commitment::L1BatchWithMetadata,
    L1BatchNumber,
};

use super::metrics::METRICS;

#[async_trait]
pub trait L1BatchPublishCriterion: fmt::Debug + Send + Sync {
    #[allow(dead_code)]
    // Takes `&self` receiver for the trait to be object-safe
    fn name(&self) -> &'static str;

    /// Returns `None` if there is no need to publish any L1 batches.
    /// Otherwise, returns the number of the last L1 batch that needs to be published.
    async fn last_l1_batch_to_publish(
        &mut self,
        storage: &mut Connection<'_, Core>,
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
        _storage: &mut Connection<'_, Core>,
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
        _storage: &mut Connection<'_, Core>,
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

#[derive(Debug, Clone, Copy)]
pub enum GasCriterionKind {
    CommitValidium,
    Execute,
}

impl From<GasCriterionKind> for AggregatedActionType {
    fn from(value: GasCriterionKind) -> Self {
        match value {
            GasCriterionKind::CommitValidium => AggregatedActionType::Commit,
            GasCriterionKind::Execute => AggregatedActionType::Execute,
        }
    }
}

#[derive(Debug)]
pub struct L1GasCriterion {
    pub gas_limit: u32,
    pub kind: GasCriterionKind,
}

impl L1GasCriterion {
    /// Base gas cost of processing aggregated `Execute` operation.
    /// It's applicable iff SL is Ethereum.
    const AGGR_L1_BATCH_EXECUTE_BASE_COST: u32 = 241_000;

    /// Base gas cost of processing aggregated `Commit` operation.
    /// It's applicable iff SL is Ethereum.
    const AGGR_L1_BATCH_COMMIT_BASE_COST: u32 = 242_000;

    /// Additional gas cost of processing `Commit` operation per batch.
    /// It's applicable iff SL is Ethereum.
    pub const L1_BATCH_COMMIT_BASE_COST: u32 = 31_000;

    pub fn new(gas_limit: u32, kind: GasCriterionKind) -> L1GasCriterion {
        L1GasCriterion { gas_limit, kind }
    }

    pub async fn total_execute_gas_amount(
        storage: &mut Connection<'_, Core>,
        batch_numbers: ops::RangeInclusive<L1BatchNumber>,
    ) -> u32 {
        let mut total = Self::AGGR_L1_BATCH_EXECUTE_BASE_COST;

        for batch_number in batch_numbers.start().0..=batch_numbers.end().0 {
            total += Self::get_execute_gas_amount(storage, batch_number.into()).await;
        }

        total
    }

    pub fn total_validium_commit_gas_amount(
        batch_numbers: ops::RangeInclusive<L1BatchNumber>,
    ) -> u32 {
        Self::AGGR_L1_BATCH_COMMIT_BASE_COST
            + (batch_numbers.end().0 - batch_numbers.start().0 + 1)
                * Self::L1_BATCH_COMMIT_BASE_COST
    }

    async fn get_execute_gas_amount(
        storage: &mut Connection<'_, Core>,
        batch_number: L1BatchNumber,
    ) -> u32 {
        let header = storage
            .blocks_dal()
            .get_l1_batch_header(batch_number)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Missing L1 batch header in DB for #{batch_number}"));

        L1_BATCH_EXECUTE_BASE_COST + u32::from(header.l1_tx_count) * L1_OPERATION_EXECUTE_COST
    }
}

#[async_trait]
impl L1BatchPublishCriterion for L1GasCriterion {
    fn name(&self) -> &'static str {
        "gas_limit"
    }

    async fn last_l1_batch_to_publish(
        &mut self,
        storage: &mut Connection<'_, Core>,
        consecutive_l1_batches: &[L1BatchWithMetadata],
        _last_sealed_l1_batch: L1BatchNumber,
    ) -> Option<L1BatchNumber> {
        let aggr_cost = match self.kind {
            GasCriterionKind::Execute => Self::AGGR_L1_BATCH_EXECUTE_BASE_COST,
            GasCriterionKind::CommitValidium => Self::AGGR_L1_BATCH_COMMIT_BASE_COST,
        };
        assert!(
            self.gas_limit > aggr_cost,
            "Config max gas cost for operations is too low"
        );
        // We're not sure our predictions are accurate, so it's safer to lower the gas limit by 10%
        let mut gas_left = (self.gas_limit as f64 * 0.9).round() as u32 - aggr_cost;

        let mut last_l1_batch = None;
        for (index, l1_batch) in consecutive_l1_batches.iter().enumerate() {
            let batch_gas = match self.kind {
                GasCriterionKind::Execute => {
                    Self::get_execute_gas_amount(storage, l1_batch.header.number).await
                }
                GasCriterionKind::CommitValidium => Self::L1_BATCH_COMMIT_BASE_COST,
            };
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
            let op: AggregatedActionType = self.kind.into();
            let first_l1_batch_number = consecutive_l1_batches.first().unwrap().header.number.0;
            tracing::debug!(
                "`gas_limit` publish criterion (gas={}) triggered for op {} with L1 batch range {:?}",
                self.gas_limit - gas_left,
                op,
                first_l1_batch_number..=last_l1_batch.0
            );
            METRICS.block_aggregation_reason[&(op, "gas").into()].inc();
        }
        last_l1_batch
    }
}
