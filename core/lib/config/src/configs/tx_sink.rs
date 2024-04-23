use std::{collections::HashSet, str::FromStr};

use serde::Deserialize;
use zksync_basic_types::Address;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TxSinkConfig {
    pub deny_list: Option<String>,
}

impl TxSinkConfig {
    pub fn deny_list(&self) -> Option<HashSet<Address>> {
        self.deny_list.as_ref().map(|list| {
            list.split(',')
                .map(|element| Address::from_str(element).unwrap())
                .collect()
        })
    }
}
