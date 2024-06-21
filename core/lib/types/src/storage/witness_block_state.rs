use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{StorageKey, StorageValue};

/// Storage data used during Witness Generation.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WitnessBlockState {
    pub read_storage_key: HashMap<StorageKey, StorageValue>,
    pub is_write_initial: HashMap<StorageKey, bool>,
}
