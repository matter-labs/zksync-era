use num::{rational::Ratio, BigInt, Zero};
use zksync_types::{web3::types::TransactionReceipt, U256};
use zksync_utils::{u256_to_biguint, UnsignedRatioSerializeAsDecimal};

#[derive(Debug, Clone)]
pub struct BlockExecutionResult {
    pub commit_result: TransactionReceipt,
    pub verify_result: TransactionReceipt,
    pub execute_result: TransactionReceipt,
}

impl BlockExecutionResult {
    pub fn new(
        commit_result: TransactionReceipt,
        verify_result: TransactionReceipt,
        execute_result: TransactionReceipt,
    ) -> Self {
        Self {
            commit_result,
            verify_result,
            execute_result,
        }
    }
}

/// Base cost of commit of one operation, we determine it by executing empty block. (with 2 noops)
#[derive(Debug, Clone)]
pub struct BaseCost {
    pub base_commit_cost: BigInt,
    pub base_verify_cost: BigInt,
    pub base_execute_cost: BigInt,
}

impl BaseCost {
    pub fn from_block_execution_result(block_execution_res: BlockExecutionResult) -> Self {
        Self {
            base_commit_cost: block_execution_res
                .commit_result
                .gas_used
                .map(|cost| u256_to_biguint(cost).into())
                .expect("commit gas used empty"),
            base_verify_cost: block_execution_res
                .verify_result
                .gas_used
                .map(|cost| u256_to_biguint(cost).into())
                .expect("verify gas used empty"),
            base_execute_cost: block_execution_res
                .execute_result
                .gas_used
                .map(|cost| u256_to_biguint(cost).into())
                .expect("execute gas used empty"),
        }
    }
}

/// Gas cost data from one test in one test we process `samples` number of operations in one block.
#[derive(Debug, Clone)]
pub struct CostsSample {
    /// number of operations in the test
    samples: usize,
    /// Total gas that user spent in this test
    users_gas_cost: BigInt,
    /// Operator commit gas cost
    commit_cost: BigInt,
    /// Operator verify gas cost
    verify_cost: BigInt,
    /// Operator execute gas cost
    execute_cost: BigInt,
}

impl CostsSample {
    pub fn new(samples: usize, users_gas_cost: U256, block_result: BlockExecutionResult) -> Self {
        Self {
            samples,
            users_gas_cost: u256_to_biguint(users_gas_cost).into(),
            commit_cost: block_result
                .commit_result
                .gas_used
                .map(|cost| u256_to_biguint(cost).into())
                .expect("commit gas used"),
            verify_cost: block_result
                .verify_result
                .gas_used
                .map(|cost| u256_to_biguint(cost).into())
                .expect("verify gas used"),
            execute_cost: block_result
                .execute_result
                .gas_used
                .map(|cost| u256_to_biguint(cost).into())
                .expect("execute gas used"),
        }
    }

    fn sub_per_operation(&self, base_cost: &BaseCost) -> CostPerOperation {
        let samples = self.samples;

        let user_gas_cost = &self.users_gas_cost / samples;

        let commit_cost = (&self.commit_cost - &base_cost.base_commit_cost) / samples;
        let verify_cost = (&self.verify_cost - &base_cost.base_verify_cost) / samples;
        let execute_cost = (&self.execute_cost - &base_cost.base_execute_cost) / samples;
        let total = &commit_cost + &verify_cost + &execute_cost;

        CostPerOperation {
            user_gas_cost,
            commit_cost,
            verify_cost,
            execute_cost,
            total,
        }
    }

    pub fn report(&self, base_cost: &BaseCost, description: &str, report_grief: bool) {
        let per_operation_cost = self.sub_per_operation(base_cost);
        per_operation_cost.report(description, report_grief);
    }
}

/// User gas cost of performing one operation and additional gas cost
/// that operator spends in each of the processing step.
///
/// # Note
///
/// * Operation cost can be negative, because some operations reclaims storage slots.
/// * Operation gas cost for some operations (e.g. Deposit) depends on sample size
#[derive(Debug, Clone)]
struct CostPerOperation {
    user_gas_cost: BigInt,
    commit_cost: BigInt,
    verify_cost: BigInt,
    execute_cost: BigInt,
    total: BigInt,
}

impl CostPerOperation {
    /// Grief factor when we neglect base commit/verify cost (when blocks are big)
    fn asymptotic_grief_factor(&self) -> String {
        let operator_total_cost_per_op = &self.commit_cost + &self.verify_cost + &self.execute_cost;

        if operator_total_cost_per_op.is_zero() {
            "0".to_string()
        } else {
            UnsignedRatioSerializeAsDecimal::serialize_to_str_with_dot(
                &Ratio::new(
                    self.user_gas_cost
                        .to_biguint()
                        .expect("user gas cost is negative"),
                    operator_total_cost_per_op
                        .to_biguint()
                        .expect("operator total cost is negative"),
                ),
                4,
            )
        }
    }

    pub fn report(&self, description: &str, report_grief: bool) {
        let grief_info = if report_grief {
            let mut info = String::from("\nuser gas cost over operator cost: ");
            info.push_str(&self.asymptotic_grief_factor());
            info
        } else {
            String::new()
        };
        println!(
            "Gas cost of {}:\nuser_gas_cost: {}\ncommit: {}\nprove: {}\nexecute: {}\ntotal: {}{}",
            description,
            self.user_gas_cost,
            self.commit_cost,
            self.verify_cost,
            self.execute_cost,
            self.total,
            grief_info
        );
        println!()
    }
}
