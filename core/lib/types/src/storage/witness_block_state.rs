use crate::{StorageKey, StorageValue, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// TODO: most likely to be discarded in favor of [`WitnessHashBlockState`].
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WitnessBlockState {
    pub read_storage_key: HashMap<StorageKey, StorageValue>,
    pub is_write_initial: HashMap<StorageKey, bool>,
}

/// Storage data used during Witness Generation.
/// By default, witness generation uses Storage Keys:
/// HashMap<StorageKey, StorageValue> and HashMap<StorageKey, bool>
/// WitnessHashBlockState is exactly the same, where we extract the has for StorageKey:
/// `storage_key.hashed_key_u256()`
#[derive(Debug, Default, PartialEq)]
pub struct WitnessHashBlockState {
    pub read_storage_key: HashMap<U256, StorageValue>,
    pub is_write_initial: HashMap<U256, bool>,
}
