use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{StorageKey, StorageValue};

/// Storage data used during Witness Generation.
#[derive(Debug, Default, Clone)]
pub struct WitnessBlockState {
    pub read_storage_key: HashMap<StorageKey, StorageValue>,
    pub is_write_initial: HashMap<StorageKey, bool>,
}

/// A serde schema for serializing/deserializing `WitnessBlockState`
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct WitnessBlockStateSerde {
    pub read_storage_key: Vec<(StorageKey, StorageValue)>,
    pub is_write_initial: Vec<(StorageKey, bool)>,
}

impl Serialize for WitnessBlockState {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WitnessBlockStateSerde {
            read_storage_key: self
                .read_storage_key
                .iter()
                .map(|(k, v)| (*k, v.clone()))
                .collect(),
            is_write_initial: self
                .is_write_initial
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect(),
        }
        .serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for WitnessBlockState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let x = WitnessBlockStateSerde::deserialize(d)?;
        Ok(Self {
            read_storage_key: x.read_storage_key.into_iter().collect(),
            is_write_initial: x.is_write_initial.into_iter().collect(),
        })
    }
}
