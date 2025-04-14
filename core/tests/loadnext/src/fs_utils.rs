//! Utilities used for reading tokens, contracts bytecode and ABI from the
//! filesystem.

use std::{fs::File, io::BufReader};

use serde::Deserialize;
use zksync_types::{network::Network, Address};
use zksync_utils::env::Workspace;

/// A token stored in `etc/tokens/{network}.json` files.
#[derive(Debug, Deserialize)]
pub struct Token {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub address: Address,
}

pub fn read_tokens(network: Network) -> anyhow::Result<Vec<Token>> {
    let home = Workspace::locate().root();
    let path = home.join(format!("etc/tokens/{network}.json"));
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    Ok(serde_json::from_reader(reader)?)
}
