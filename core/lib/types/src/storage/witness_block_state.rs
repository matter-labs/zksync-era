use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{StorageKey, StorageValue};

/// Storage data used during Witness Generation.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WitnessBlockState {
    pub read_storage_key: HashMap<StorageKey, StorageValue>,
    pub is_write_initial: HashMap<StorageKey, bool>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WitnessBlockStateSerializable {
    pub read_storage_key: Vec<(StorageKey, StorageValue)>,
    pub is_write_initial: Vec<(StorageKey, bool)>,
}

impl From<WitnessBlockState> for WitnessBlockStateSerializable {
    fn from(state: WitnessBlockState) -> Self {
        Self {
            read_storage_key: state.read_storage_key.into_iter().collect(),
            is_write_initial: state.is_write_initial.into_iter().collect(),
        }
    }
}

impl From<WitnessBlockStateSerializable> for WitnessBlockState {
    fn from(state: WitnessBlockStateSerializable) -> Self {
        Self {
            read_storage_key: state.read_storage_key.into_iter().collect(),
            is_write_initial: state.is_write_initial.into_iter().collect(),
        }
    }
}
