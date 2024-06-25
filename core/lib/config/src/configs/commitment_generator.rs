use std::num::NonZeroU32;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CommitmentGeneratorConfig {
    /// Maximum degree of parallelism during commitment generation, i.e., the maximum number of L1 batches being processed in parallel.
    /// If not specified, commitment generator will use a value roughly equal to the number of CPU cores with some clamping applied.
    pub max_parallelism: NonZeroU32,
}
