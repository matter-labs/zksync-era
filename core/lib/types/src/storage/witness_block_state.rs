use crate::{StorageKey, StorageValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Storage data used during Witness Generation.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WitnessBlockState {
    pub read_storage_key: HashMap<StorageKey, StorageValue>,
    pub is_write_initial: HashMap<StorageKey, bool>,
}
