use std::{collections::HashSet, str::FromStr};

use serde::Deserialize;
use zksync_basic_types::Address;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TxSinkConfig {
    pub allow_list: Option<String>,
}

impl TxSinkConfig {
    pub fn allow_list(&self) -> Option<HashSet<Address>> {
        // Return deny list is not set or empty
        if self.allow_list.is_none() || self.allow_list.as_ref().unwrap().is_empty() {
            return None;
        }

        // Parse deny list from a string and return it as a set of addresses
        // Example: "0x1,0x2,0x3" -> { Address::from_str("0x1").unwrap(), Address::from_str("0x2").unwrap(), Address::from_str("0x3").unwrap() }
        // Note: This assumes that the addresses are separated by commas and are in valid format.
        self.allow_list.as_ref().map(|list| {
            list.split(',')
                .map(|element| Address::from_str(element).unwrap())
                .collect()
        })
    }
}
