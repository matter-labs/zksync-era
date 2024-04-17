use zksync_system_constants::{
    AGGR_L1_BATCH_COMMIT_BASE_COST, AGGR_L1_BATCH_EXECUTE_BASE_COST, AGGR_L1_BATCH_PROVE_BASE_COST,
};
use zksync_types::aggregated_operations::AggregatedActionType;

pub fn agg_l1_batch_base_cost(op: AggregatedActionType) -> u32 {
    match op {
        AggregatedActionType::Commit => AGGR_L1_BATCH_COMMIT_BASE_COST,
        AggregatedActionType::PublishProofOnchain => AGGR_L1_BATCH_PROVE_BASE_COST,
        AggregatedActionType::Execute => AGGR_L1_BATCH_EXECUTE_BASE_COST,
    }
}
