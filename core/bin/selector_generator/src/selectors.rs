use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Short (4-byte) function selector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
struct Selector(String);

/// Function name without parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
struct FunctionName(String);

/// A set of function selectors and their corresponding function names.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Selectors {
    #[serde(flatten)]
    selectors: HashMap<Selector, FunctionName>,
}

impl Selectors {
    /// Loads the selectors from the file, or returns a new instance if the file is a valid
    /// JSON, but doesn't contain `ABI` section.
    ///
    /// Will return an error if file doesn't exist or cannot be deserialized.
    pub async fn load(file_path: &PathBuf) -> anyhow::Result<Selectors> {
        let file = tokio::fs::read(file_path)
            .await
            .context("Failed to read file")?;
        let json: serde_json::Value =
            serde_json::from_slice(&file).context("Failed to deserialize file")?;
        let Some(abi) = json.get("abi").cloned() else {
            return Ok(Selectors::default());
        };

        let contract: ethabi::Contract =
            serde_json::from_value(abi).context("Failed to parse abi")?;
        Ok(Self::new(contract))
    }

    /// Loads selectors from a given contract.
    pub fn new(contract: ethabi::Contract) -> Self {
        let selectors: HashMap<_, _> = contract
            .functions
            .into_values()
            .flatten()
            .map(|function| {
                let selector = hex::encode(function.short_signature());
                (Selector(selector), FunctionName(function.name))
            })
            .collect();
        Self { selectors }
    }

    /// Merges new selectors into the existing set.
    pub fn merge(&mut self, new: Self) {
        for (selector, name) in new.selectors {
            self.selectors
                .entry(selector.clone())
                .and_modify(|e| {
                    assert_eq!(
                        e, &name,
                        "Function name mismatch for selector '{:?}'",
                        selector
                    )
                })
                .or_insert(name);
        }
    }

    pub fn len(&self) -> usize {
        self.selectors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selectors() {
        let contract_json = r#"[
            {
                "type": "function",
                "name": "transfer",
                "inputs": [
                    { "name": "to", "type": "address" },
                    { "name": "value", "type": "uint256" }
                ],
                "outputs": [],
                "stateMutability": "nonpayable"
            },
            {
                "type": "function",
                "name": "bar",
                "inputs": [],
                "outputs": [],
                "stateMutability": "nonpayable"
            }
        ]
        "#;

        let contract: ethabi::Contract = serde_json::from_str(contract_json).unwrap();
        let selectors = Selectors::new(contract);
        assert_eq!(selectors.len(), 2);

        // Check the generated selectors.
        assert_eq!(
            selectors
                .selectors
                .get(&Selector("a9059cbb".to_string()))
                .expect("No selector for transfer found"),
            &FunctionName("transfer".to_string())
        );
    }
}
