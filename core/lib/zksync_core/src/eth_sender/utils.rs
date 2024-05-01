use zksync_types::aggregated_operations::AggregatedActionType;

// TODO(QIT-32): Remove constants(except `L1_OPERATION_EXECUTE_COST`) and logic that use them
const AGGR_L1_BATCH_COMMIT_BASE_COST: u32 = 242_000;
const AGGR_L1_BATCH_PROVE_BASE_COST: u32 = 1_000_000;
const AGGR_L1_BATCH_EXECUTE_BASE_COST: u32 = 241_000;

pub fn agg_l1_batch_base_cost(op: AggregatedActionType) -> u32 {
    match op {
        AggregatedActionType::Commit => AGGR_L1_BATCH_COMMIT_BASE_COST,
        AggregatedActionType::PublishProofOnchain => AGGR_L1_BATCH_PROVE_BASE_COST,
        AggregatedActionType::Execute => AGGR_L1_BATCH_EXECUTE_BASE_COST,
    }
}
