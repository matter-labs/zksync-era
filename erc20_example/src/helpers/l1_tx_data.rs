use std::fmt::Display;

use colored::Colorize;
use ethers::abi::Hash;
use zksync_types::U256;

use super::TxType;

pub struct L1TxData {
    pub tx_type: TxType,
    pub hash: Hash,
    pub gas_used: U256,
}

impl Display for L1TxData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{formatted_tx_type} tx",
            formatted_tx_type = format!("{:?}", self.tx_type).bright_blue().bold()
        )?;
        writeln!(
            f,
            "{formatter_label}: {formatted_hash}",
            formatter_label = "Hash".bold(),
            formatted_hash = format!("{:?}", self.hash).bright_green()
        )?;
        writeln!(
            f,
            "{formatter_label}: {formatted_gas_used}",
            formatter_label = "Gas used".bold(),
            formatted_gas_used = format!("{:#?}", self.gas_used).bright_cyan()
        )?;
        Ok(())
    }
}
