use serde::Deserialize;
use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ConsistencyCheckerConfig {
    /// Maximum number of batches to recheck.
    #[config(default_t = 10)]
    pub max_batches_to_recheck: u32,
}
