// Currently, every AGGR_* cost is overestimated,
// so there are safety margins around 100_000 -- 200_000

pub const AGGR_BLOCK_COMMIT_BASE_COST: u32 = 242_000;
pub const AGGR_BLOCK_PROVE_BASE_COST: u32 = 1_000_000;
pub const AGGR_BLOCK_EXECUTE_BASE_COST: u32 = 241_000;

pub const BLOCK_COMMIT_BASE_COST: u32 = 31_000;
pub const BLOCK_PROVE_BASE_COST: u32 = 7_000;
pub const BLOCK_EXECUTE_BASE_COST: u32 = 30_000;

pub const EXECUTE_COMMIT_COST: u32 = 0;
pub const EXECUTE_EXECUTE_COST: u32 = 0;

pub const L1_OPERATION_EXECUTE_COST: u32 = 12_500;

pub const GAS_PER_BYTE: u32 = 18;
