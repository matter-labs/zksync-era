use std::num::NonZeroU32;

use smart_config::{DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct CommitmentGeneratorConfig {
    /// Maximum degree of parallelism during commitment generation, i.e., the maximum number of L1 batches being processed in parallel.
    /// If not specified, commitment generator will use a value roughly equal to the number of CPU cores with some clamping applied.
    pub max_parallelism: Option<NonZeroU32>,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    fn expected_config() -> CommitmentGeneratorConfig {
        CommitmentGeneratorConfig {
            max_parallelism: Some(NonZeroU32::new(10).unwrap()),
        }
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          max_parallelism: 10
        "#;

        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: CommitmentGeneratorConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
