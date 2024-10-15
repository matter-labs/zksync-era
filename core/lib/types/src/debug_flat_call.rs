use serde::{Deserialize, Serialize};
use zksync_basic_types::{web3::Bytes, U256};

use crate::{api::DebugCallType, Address, H256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultDebugCallFlat {
    pub tx_hash: H256,
    pub result: Vec<DebugCallFlat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugCallFlat {
    pub action: Action,
    pub result: Option<CallResult>,
    pub subtraces: usize,
    pub trace_address: Vec<usize>,
    pub transaction_position: usize,
    pub transaction_hash: H256,
    pub block_number: u32,
    pub block_hash: H256,
    pub r#type: DebugCallType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub call_type: DebugCallType,
    pub from: Address,
    pub to: Address,
    pub gas: U256,
    pub value: U256,
    pub input: Bytes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallResult {
    pub output: Bytes,
    pub gas_used: U256,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CallTraceMeta {
    pub index_in_block: usize,
    pub tx_hash: H256,
    pub block_number: u32,
    pub block_hash: H256,
}
