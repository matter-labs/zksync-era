use crate::Bytes;
use serde::{Deserialize, Serialize};

#[repr(i32)]
pub enum MulticallErrCode {
    FastFail = -40015,
    CallFail = -40012,
}

/// MultiCall
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct MultiCallResp {
    /// Result
    pub results: Vec<CallResult>,
    /// Stats
    pub stats: MultiCallStats,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct CallResult {
    /// Code
    pub code: i32,
    /// Err
    pub err: String,
    /// Result
    pub result: Bytes,
    /// GasUsed
    pub gas_used: i64,
    /// TimeCost
    pub time_cost: f64,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct MultiCallStats {
    /// BlockNum
    pub block_num: i64,
    /// BlockTime
    pub block_time: i64,
    /// Success
    pub success: bool,
}
