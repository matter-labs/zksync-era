use std::{collections::HashSet, str::FromStr};

use zksync_basic_types::Address;

#[derive(Debug, Clone, PartialEq)]
pub struct TxSinkConfig {
    /// Amount of confirmations for the priority operation to be processed.
    /// If not specified operation will be processed once its block is finalized.
    pub deny_list: Option<String>,
}

impl TxSinkConfig {
    /// Converts `self.deny_list` into `HashSet<Address>`.
    pub fn deny_list(&self) -> Option<HashSet<Address>> {
        if let Some(list) = &self.deny_list {
            Some(
                list.split(',')
                    .map(|element| Address::from_str(element).unwrap())
                    .collect(),
            )
        } else {
            None
        }
    }
}
