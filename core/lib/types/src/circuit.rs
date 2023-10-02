use zkevm_test_harness::geometry_config::get_geometry_config;
use zkevm_test_harness::toolset::GeometryConfig;

pub const LEAF_SPLITTING_FACTOR: usize = 50;
pub const NODE_SPLITTING_FACTOR: usize = 48;

/// Max number of basic circuits per L1 batch.
pub const SCHEDULER_UPPER_BOUND: u32 = (LEAF_SPLITTING_FACTOR * NODE_SPLITTING_FACTOR) as u32;

pub const LEAF_CIRCUIT_INDEX: u8 = 2;
pub const NODE_CIRCUIT_INDEX: u8 = 1;
pub const SCHEDULER_CIRCUIT_INDEX: u8 = 0;

pub const GEOMETRY_CONFIG: GeometryConfig = get_geometry_config();
