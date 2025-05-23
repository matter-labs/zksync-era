use clap::Parser;
use zksync_basic_types::{Address, U256};

use crate::defaults::LOCAL_RPC_URL;

// This is the address that we configured in RETH config to hold a lot of eth in L1.
const L1_RICH_ACCOUNT_PK: &str =
    "0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110";
const L1_RICH_ACCOUNT_ADDRESS: &str = "0x36615Cf349d7F6344891B1e7CA7C72883F5dc049";

// If nothing is set, simply transfer 1 ETH from L1 to L2
#[derive(Debug, Parser)]
pub struct RichAccountArgs {
    /// L2 Account to send funds to (default same as L1 rich account).
    #[clap()]
    pub l2_account: Option<Address>,
    /// L1 private key to send funds from (default: Reth rich account)
    #[clap(long)]
    pub l1_account_private_key: Option<String>,
    /// Amount (default 1 ETH).
    #[clap(long)]
    pub amount: Option<U256>,
    /// L1 RPC URL (default: localhost reth).
    #[clap(long)]
    pub l1_rpc_url: Option<String>,
}

#[derive(Debug)]
pub struct RichAccountArgsFinal {
    pub l2_account: Address,
    pub l1_account_private_key: String,
    pub amount: U256,
    pub l1_rpc_url: String,
}

impl RichAccountArgs {
    pub fn fill_values_with_prompt(self) -> RichAccountArgsFinal {
        let l2_account = self
            .l2_account
            .unwrap_or_else(|| L1_RICH_ACCOUNT_ADDRESS.parse().unwrap());

        let l1_account_private_key = self
            .l1_account_private_key
            .unwrap_or(L1_RICH_ACCOUNT_PK.to_string());

        let amount = self
            .amount
            .unwrap_or(U256::from(1_000_000_000_000_000_000u64));
        let l1_rpc_url: String = self.l1_rpc_url.unwrap_or_else(|| LOCAL_RPC_URL.to_string());

        RichAccountArgsFinal {
            l2_account,
            l1_account_private_key,
            amount,
            l1_rpc_url,
        }
    }
}
