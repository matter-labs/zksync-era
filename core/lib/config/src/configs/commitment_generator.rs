use std::num::NonZeroU32;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CommitmentGeneratorConfig {
    pub max_parallelism: NonZeroU32,
}
