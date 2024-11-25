use std::num::NonZeroU32;

use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct CommitmentGeneratorConfig {
    /// Maximum degree of parallelism during commitment generation, i.e., the maximum number of L1 batches being processed in parallel.
    /// If not specified, commitment generator will use a value roughly equal to the number of CPU cores with some clamping applied.
    // FIXME: remove the corresponding experimental param
    pub max_parallelism: Option<NonZeroU32>,
}
