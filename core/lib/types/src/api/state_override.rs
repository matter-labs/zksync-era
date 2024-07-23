use std::{collections::HashMap, ops::Deref};

use serde::{Deserialize, Deserializer, Serialize};
use zksync_basic_types::{web3::Bytes, H256, U256};

use crate::Address;

/// Collection of overridden accounts
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateOverride(HashMap<Address, OverrideAccount>);

/// Account override for `eth_estimateGas`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideAccount {
    pub balance: Option<U256>,
    pub nonce: Option<U256>,
    pub code: Option<Bytes>,
    #[serde(flatten, deserialize_with = "state_deserializer")]
    pub state: Option<OverrideState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverrideState {
    State(HashMap<H256, H256>),
    StateDiff(HashMap<H256, H256>),
}

fn state_deserializer<'de, D>(deserializer: D) -> Result<Option<OverrideState>, D::Error>
where
    D: Deserializer<'de>,
{
    let val = serde_json::Value::deserialize(deserializer)?;
    let state: Option<HashMap<H256, H256>> = match val.get("state") {
        Some(val) => serde_json::from_value(val.clone()).map_err(serde::de::Error::custom)?,
        None => None,
    };
    let state_diff: Option<HashMap<H256, H256>> = match val.get("stateDiff") {
        Some(val) => serde_json::from_value(val.clone()).map_err(serde::de::Error::custom)?,
        None => None,
    };

    match (state, state_diff) {
        (Some(state), None) => Ok(Some(OverrideState::State(state))),
        (None, Some(state_diff)) => Ok(Some(OverrideState::StateDiff(state_diff))),
        (None, None) => Ok(None),
        _ => Err(serde::de::Error::custom(
            "Both 'state' and 'stateDiff' cannot be set simultaneously",
        )),
    }
}

impl StateOverride {
    pub fn new(state: HashMap<Address, OverrideAccount>) -> Self {
        Self(state)
    }

    pub fn get(&self, address: &Address) -> Option<&OverrideAccount> {
        self.0.get(address)
    }
}

impl Deref for StateOverride {
    type Target = HashMap<Address, OverrideAccount>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
