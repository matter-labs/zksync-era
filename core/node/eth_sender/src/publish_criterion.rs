use std::{fmt, ops, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::{
    aggregated_operations::L1BatchAggregatedActionType, commitment::L1BatchWithMetadata,
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
        is_gateway: bool,
    ) -> Option<L1BatchNumber>;
}

#[derive(Debug)]
pub struct NumberCriterion {
    pub op: L1BatchAggregatedActionType,
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
        _is_gateway: bool,
    ) -> Option<L1BatchNumber> {
        let mut batch_numbers = consecutive_l1_batches
            .iter()
            .map(|batch| batch.header.number.0);

        let first = batch_numbers.next()?;
        let last_batch_number = batch_numbers.next_back().unwrap_or(first);
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
    pub op: L1BatchAggregatedActionType,
    /// Maximum L1 batch age in seconds. Once reached, we pack and publish all the available L1 batches.
    pub deadline: Duration,
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
        _is_gateway: bool,
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
        if oldest_l1_batch_age_seconds >= self.deadline.as_secs() {
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

impl From<GasCriterionKind> for L1BatchAggregatedActionType {
    fn from(value: GasCriterionKind) -> Self {
        match value {
            GasCriterionKind::CommitValidium => L1BatchAggregatedActionType::Commit,
            GasCriterionKind::Execute => L1BatchAggregatedActionType::Execute,
        }
    }
}

#[derive(Debug)]
pub struct L1GasCriterion {
    pub gas_limit: u64,
    pub kind: GasCriterionKind,
}

impl L1GasCriterion {
    pub fn new(gas_limit: u64, kind: GasCriterionKind) -> L1GasCriterion {
        L1GasCriterion { gas_limit, kind }
    }

    pub async fn total_execute_gas_amount(
        storage: &mut Connection<'_, Core>,
        batch_numbers: ops::RangeInclusive<L1BatchNumber>,
        is_gateway: bool,
    ) -> u64 {
        let costs = GasConsts::execute_costs(is_gateway);
        let mut total = costs.base;

        for batch_number in batch_numbers.start().0..=batch_numbers.end().0 {
            total += Self::get_execute_gas_amount(storage, batch_number.into(), &costs).await;
        }

        total
    }

    pub fn total_precommit_gas_amount(is_gateway: bool, txs_len: usize) -> u64 {
        let costs = GasConsts::precommit_costs(is_gateway);
        costs.base + costs.per_tx * txs_len as u64
    }

    pub fn total_proof_gas_amount(is_gateway: bool) -> u64 {
        GasConsts::proof_costs(is_gateway)
    }

    // Return the gas limit for the Validium part of commit.
    // Gas limit for DA part will be adjusted later in eth_tx_manager,
    // when the pubdata price will be known
    pub fn total_commit_validium_gas_amount(
        batch_numbers: ops::RangeInclusive<L1BatchNumber>,
        is_gateway: bool,
    ) -> u64 {
        let costs = GasConsts::commit_costs(is_gateway);
        costs.base
            + ((batch_numbers.end().0 - batch_numbers.start().0 + 1) as u64) * costs.per_batch
    }

    async fn get_execute_gas_amount(
        storage: &mut Connection<'_, Core>,
        batch_number: L1BatchNumber,
        costs: &ExecuteCosts,
    ) -> u64 {
        let header = storage
            .blocks_dal()
            .get_l1_batch_header(batch_number)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Missing L1 batch header in DB for #{batch_number}"));

        costs.per_batch + u64::from(header.l1_tx_count) * costs.per_l1_l2_tx
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
        is_gateway: bool,
    ) -> Option<L1BatchNumber> {
        let execute_costs = GasConsts::execute_costs(is_gateway);
        let commit_costs = GasConsts::commit_costs(is_gateway);

        let aggr_cost = match self.kind {
            GasCriterionKind::Execute => execute_costs.base,
            GasCriterionKind::CommitValidium => commit_costs.base,
        };
        assert!(
            self.gas_limit > aggr_cost,
            "Config max gas cost for operations is too low"
        );
        // We're not sure our predictions are accurate, so it's safer to lower the gas limit by 10%
        let mut gas_left = (self.gas_limit as f64 * 0.9).round() as u64 - aggr_cost;

        let mut last_l1_batch = None;
        for (index, l1_batch) in consecutive_l1_batches.iter().enumerate() {
            let batch_gas = match self.kind {
                GasCriterionKind::Execute => {
                    Self::get_execute_gas_amount(storage, l1_batch.header.number, &execute_costs)
                        .await
                }
                GasCriterionKind::CommitValidium => commit_costs.per_batch,
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
            let op: L1BatchAggregatedActionType = self.kind.into();
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

#[derive(Debug)]
struct GasConsts;

#[derive(Debug)]
struct CommitGasConsts {
    base: u64,
    per_batch: u64,
}

#[derive(Debug)]
struct ExecuteCosts {
    base: u64,
    per_batch: u64,
    per_l1_l2_tx: u64,
}

#[derive(Debug)]
struct PrecommitCosts {
    base: u64,
    per_tx: u64,
}

impl GasConsts {
    /// Base gas cost of processing aggregated `Execute` operation.
    /// It's applicable iff SL is Ethereum.
    const AGGR_L1_BATCH_EXECUTE_BASE_COST: u64 = 200_000;
    /// Base gas cost of processing aggregated `Execute` operation.
    /// It's applicable if SL is  Gateway.
    const AGGR_GATEWAY_BATCH_EXECUTE_BASE_COST: u64 = 300_000;

    /// Base gas cost of processing aggregated `Commit` operation.
    /// It's applicable if SL is Ethereum.
    const AGGR_L1_BATCH_COMMIT_BASE_COST: u64 = 242_000;

    /// Additional gas cost of processing `Commit` operation per batch.
    /// It's applicable if SL is Ethereum.
    const L1_BATCH_COMMIT_BASE_COST: u64 = 31_000;

    /// Additional gas cost of processing `Commit` operation per batch.
    /// It's applicable if SL is Gateway.
    const AGGR_GATEWAY_BATCH_COMMIT_BASE_COST: u64 = 150_000;

    /// Additional gas cost of processing `Commit` operation per batch.
    /// It's applicable if SL is Gateway.
    const GATEWAY_BATCH_COMMIT_BASE_COST: u64 = 200_000;

    /// All gas cost of processing `PROVE` operation per batch.
    /// It's applicable if SL is GATEWAY.
    /// TODO calculate it properly
    const GATEWAY_BATCH_PROOF_GAS_COST: u64 = 1_600_000;

    /// All gas cost of processing `PROVE` operation per batch.
    /// It's applicable if SL is Ethereum.
    const L1_BATCH_PROOF_GAS_COST_ETHEREUM: u64 = 800_000;

    /// Base gas cost of processing `EXECUTION` operation per batch.
    /// It's applicable if SL is GATEWAY.
    const GATEWAY_BATCH_EXECUTION_COST: u64 = 100_000;
    /// Gas cost of processing `l1_operation` in batch.
    /// It's applicable if SL is GATEWAY.
    const GATEWAY_L1_OPERATION_COST: u64 = 4_000;

    /// Additional gas cost of processing `Execute` operation per batch.
    /// It's applicable iff SL is Ethereum.
    const L1_BATCH_EXECUTE_BASE_COST: u64 = 50_000;

    /// Additional gas cost of processing `Execute` operation per L1->L2 tx.
    /// It's applicable iff SL is Ethereum.
    const L1_OPERATION_EXECUTE_COST: u64 = 15_000;

    /// Base gas cost of processing `Precommit` operation.
    const PRECOMMIT_BASE_COST: u64 = 200_000;

    /// Additional gas cost of processing `Precommit` operation per tx.
    const PRECOMMIT_PER_TX_COST: u64 = 10_000;

    fn commit_costs(is_gateway: bool) -> CommitGasConsts {
        if is_gateway {
            CommitGasConsts {
                base: Self::GATEWAY_BATCH_COMMIT_BASE_COST,
                per_batch: Self::AGGR_GATEWAY_BATCH_COMMIT_BASE_COST,
            }
        } else {
            CommitGasConsts {
                base: Self::L1_BATCH_COMMIT_BASE_COST,
                per_batch: Self::AGGR_L1_BATCH_COMMIT_BASE_COST,
            }
        }
    }

    fn proof_costs(is_gateway: bool) -> u64 {
        if is_gateway {
            Self::GATEWAY_BATCH_PROOF_GAS_COST
        } else {
            Self::L1_BATCH_PROOF_GAS_COST_ETHEREUM
        }
    }

    fn precommit_costs(is_gateway: bool) -> PrecommitCosts {
        let mut costs = PrecommitCosts {
            base: Self::PRECOMMIT_BASE_COST,
            per_tx: Self::PRECOMMIT_PER_TX_COST,
        };
        if is_gateway {
            costs.base *= 2;
            costs.per_tx *= 2;
        }
        costs
    }

    fn execute_costs(is_gateway: bool) -> ExecuteCosts {
        if is_gateway {
            ExecuteCosts {
                base: Self::AGGR_GATEWAY_BATCH_EXECUTE_BASE_COST,
                per_batch: Self::GATEWAY_BATCH_EXECUTION_COST,
                per_l1_l2_tx: Self::GATEWAY_L1_OPERATION_COST,
            }
        } else {
            ExecuteCosts {
                base: Self::AGGR_L1_BATCH_EXECUTE_BASE_COST,
                per_batch: Self::L1_BATCH_EXECUTE_BASE_COST,
                per_l1_l2_tx: Self::L1_OPERATION_EXECUTE_COST,
            }
        }
    }
}
