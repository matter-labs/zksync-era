use serde::de::{Deserializer, Error, MapAccess, Unexpected, Visitor};
use std::{collections::HashMap, fmt};
use zksync_contracts::BaseSystemContractsHashes;

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{api::Log, Address, Bytes, Execute, L1BatchNumber, MiniblockNumber, Nonce, H256, U256};

use serde_with::rust::display_fromstr::deserialize as deserialize_fromstr;

pub use crate::Execute as ExecuteData;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum PaginationDirection {
    Newer,
    Older,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct PaginationQuery {
    // There is known problem with serde flatten and serde_urlencoded.
    // It is described here https://github.com/nox/serde_urlencoded/issues/33
    // A workaround is described here https://docs.rs/serde_qs/0.9.1/serde_qs/index.html#flatten-workaround.
    // It includes using of `deserialize_with`
    #[serde(deserialize_with = "deserialize_fromstr")]
    pub limit: usize,
    #[serde(deserialize_with = "deserialize_fromstr", default)]
    pub offset: usize,
    pub direction: PaginationDirection,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlocksQuery {
    pub from: Option<MiniblockNumber>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct L1BatchesQuery {
    pub from: Option<L1BatchNumber>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Debug, Clone, Copy)]
pub struct TxPosition {
    pub block_number: MiniblockNumber,
    pub tx_index: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsQuery {
    pub from_block_number: Option<MiniblockNumber>,
    pub from_tx_index: Option<u32>,
    pub block_number: Option<MiniblockNumber>,
    pub l1_batch_number: Option<L1BatchNumber>,
    pub address: Option<Address>,
    pub account_address: Option<Address>,
    pub contract_address: Option<Address>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

impl TransactionsQuery {
    pub fn tx_position(&self) -> Option<TxPosition> {
        self.from_block_number.map(|block_number| TxPosition {
            block_number,
            tx_index: self.from_tx_index,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsResponse {
    pub list: Vec<TransactionDetails>,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct EventsQuery {
    pub from_block_number: Option<MiniblockNumber>,
    pub contract_address: Option<Address>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventsResponse {
    pub list: Vec<Log>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransactionData {
    Execute(ExecuteData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockStatus {
    Sealed,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionStatus {
    Pending,
    Included,
    Verified,
    Failed,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockPageItem {
    pub number: MiniblockNumber,
    pub l1_tx_count: usize,
    pub l2_tx_count: usize,
    pub hash: Option<H256>,
    pub status: BlockStatus,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionResponse {
    #[serde(flatten)]
    pub tx: TransactionDetails,
    pub logs: Vec<Log>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetails {
    pub transaction_hash: H256,
    pub data: Execute,
    pub is_l1_originated: bool,
    pub status: TransactionStatus,
    pub fee: U256,
    pub nonce: Option<Nonce>,
    pub block_number: Option<MiniblockNumber>,
    pub l1_batch_number: Option<L1BatchNumber>,
    pub block_hash: Option<H256>,
    pub index_in_block: Option<u32>,
    pub initiator_address: Address,
    pub received_at: DateTime<Utc>,
    pub miniblock_timestamp: Option<u64>,
    pub eth_commit_tx_hash: Option<H256>,
    pub eth_prove_tx_hash: Option<H256>,
    pub eth_execute_tx_hash: Option<H256>,
    pub erc20_transfers: Vec<Erc20TransferInfo>,
    /// It is `Some` only if the transaction calls `transfer` method of some ERC20 token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer: Option<Erc20TransferInfo>,
    pub balance_changes: Vec<BalanceChangeInfo>,
    pub r#type: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Erc20TransferInfo {
    pub token_info: ExplorerTokenInfo,
    pub from: Address,
    pub to: Address,
    pub amount: U256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BalanceChangeType {
    Transfer,
    Deposit,
    Withdrawal,
    Fee,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceChangeInfo {
    pub token_info: ExplorerTokenInfo,
    pub from: Address,
    pub to: Address,
    pub amount: U256,
    pub r#type: BalanceChangeType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerTokenInfo {
    pub l1_address: Address,
    pub l2_address: Address,
    pub address: Address,
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub usd_price: Option<BigDecimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceItem {
    pub token_info: ExplorerTokenInfo,
    pub balance: U256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountType {
    EOA,
    Contract,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDetails {
    pub address: Address,
    pub balances: HashMap<Address, BalanceItem>,
    pub sealed_nonce: Nonce,
    pub verified_nonce: Nonce,
    pub account_type: AccountType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractDetails {
    #[serde(flatten)]
    pub info: ContractBasicInfo,
    #[serde(flatten)]
    pub stats: ContractStats,
    pub balances: HashMap<Address, BalanceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "info")]
pub enum AddressDetails {
    Account(AccountDetails),
    Contract(ContractDetails),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContractStats {
    pub total_transactions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractBasicInfo {
    pub address: Address,
    pub bytecode: Bytes,
    pub creator_address: Address,
    pub creator_tx_hash: H256,
    pub created_in_block_number: MiniblockNumber,
    pub verification_info: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockDetails {
    pub number: MiniblockNumber,
    pub l1_batch_number: L1BatchNumber,
    pub timestamp: u64,
    pub l1_tx_count: usize,
    pub l2_tx_count: usize,
    pub root_hash: Option<H256>,
    pub status: BlockStatus,
    pub commit_tx_hash: Option<H256>,
    pub committed_at: Option<DateTime<Utc>>,
    pub prove_tx_hash: Option<H256>,
    pub proven_at: Option<DateTime<Utc>>,
    pub execute_tx_hash: Option<H256>,
    pub executed_at: Option<DateTime<Utc>>,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub operator_address: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1BatchDetails {
    pub number: L1BatchNumber,
    pub timestamp: u64,
    pub l1_tx_count: usize,
    pub l2_tx_count: usize,
    pub root_hash: Option<H256>,
    pub status: BlockStatus,
    pub commit_tx_hash: Option<H256>,
    pub committed_at: Option<DateTime<Utc>>,
    pub prove_tx_hash: Option<H256>,
    pub proven_at: Option<DateTime<Utc>>,
    pub execute_tx_hash: Option<H256>,
    pub executed_at: Option<DateTime<Utc>>,
    pub l1_gas_price: u64,
    pub l2_fair_gas_price: u64,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct L1BatchPageItem {
    pub number: L1BatchNumber,
    pub timestamp: u64,
    pub l1_tx_count: usize,
    pub l2_tx_count: usize,
    pub root_hash: Option<H256>,
    pub status: BlockStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "codeFormat", content = "sourceCode")]
pub enum SourceCodeData {
    #[serde(rename = "solidity-single-file")]
    SolSingleFile(String),
    #[serde(rename = "solidity-standard-json-input")]
    StandardJsonInput(serde_json::Map<String, serde_json::Value>),
    #[serde(rename = "yul-single-file")]
    YulSingleFile(String),
}

// Implementing Custom deserializer which deserializes `SourceCodeData`
// as `SingleFile` if `codeFormat` is not specified.
// Serde doesn't support this feature: https://github.com/serde-rs/serde/issues/2231
impl<'de> Deserialize<'de> for SourceCodeData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(SourceCodeVisitor)
    }
}

struct SourceCodeVisitor;

impl<'de> Visitor<'de> for SourceCodeVisitor {
    type Value = SourceCodeData;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("source code data")
    }
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut source_code = None;
        let mut r#type = None;
        while let Some(key) = map.next_key::<String>()? {
            match &*key {
                "sourceCode" => source_code = Some(map.next_value::<serde_json::Value>()?),
                "codeFormat" => r#type = Some(map.next_value::<String>()?),
                _ => continue,
            }
        }
        let result = match r#type.as_deref() {
            Some("solidity-single-file") | None => {
                let value = source_code.ok_or_else(|| A::Error::missing_field("source_code"))?;
                SourceCodeData::SolSingleFile(
                    value
                        .as_str()
                        .ok_or_else(|| {
                            A::Error::invalid_type(Unexpected::Other(&value.to_string()), &self)
                        })?
                        .to_string(),
                )
            }
            Some("yul-single-file") => {
                let value = source_code.ok_or_else(|| A::Error::missing_field("source_code"))?;
                SourceCodeData::YulSingleFile(
                    value
                        .as_str()
                        .ok_or_else(|| {
                            A::Error::invalid_type(Unexpected::Other(&value.to_string()), &self)
                        })?
                        .to_string(),
                )
            }
            Some("solidity-standard-json-input") => {
                let value = source_code.ok_or_else(|| A::Error::missing_field("source_code"))?;
                SourceCodeData::StandardJsonInput(
                    value
                        .as_object()
                        .ok_or_else(|| {
                            A::Error::invalid_type(Unexpected::Other(&value.to_string()), &self)
                        })?
                        .clone(),
                )
            }
            Some(x) => {
                return Err(A::Error::unknown_variant(
                    x,
                    &[
                        "solidity-single-file",
                        "solidity-standard-json-input",
                        "yul-single-file",
                    ],
                ))
            }
        };
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationIncomingRequest {
    pub contract_address: Address,
    #[serde(flatten)]
    pub source_code_data: SourceCodeData,
    pub contract_name: String,
    pub compiler_zksolc_version: String,
    pub compiler_solc_version: String,
    pub optimization_used: bool,
    #[serde(default)]
    pub constructor_arguments: Bytes,
    #[serde(default)]
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationRequest {
    pub id: usize,
    #[serde(flatten)]
    pub req: VerificationIncomingRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilationArtifacts {
    pub bytecode: Vec<u8>,
    pub abi: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationInfo {
    pub request: VerificationRequest,
    pub artifacts: CompilationArtifacts,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationRequestStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compilation_errors: Option<Vec<String>>,
}

#[derive(Debug)]
pub enum DeployContractCalldata {
    Deploy(Vec<u8>),
    Ignore,
}

#[cfg(test)]
mod tests {
    use super::SourceCodeData;

    #[test]
    fn source_code_deserialization() {
        let single_file_str = r#"{"codeFormat": "solidity-single-file", "sourceCode": "text"}"#;
        let single_file_result = serde_json::from_str::<SourceCodeData>(single_file_str);
        assert!(matches!(
            single_file_result,
            Ok(SourceCodeData::SolSingleFile(_))
        ));

        let stand_json_input_str =
            r#"{"codeFormat": "solidity-standard-json-input", "sourceCode": {}}"#;
        let stand_json_input_result = serde_json::from_str::<SourceCodeData>(stand_json_input_str);
        assert!(matches!(
            stand_json_input_result,
            Ok(SourceCodeData::StandardJsonInput(_))
        ));

        let type_not_specified_str = r#"{"sourceCode": "text"}"#;
        let type_not_specified_result =
            serde_json::from_str::<SourceCodeData>(type_not_specified_str);
        assert!(matches!(
            type_not_specified_result,
            Ok(SourceCodeData::SolSingleFile(_))
        ));

        let type_not_specified_object_str = r#"{"sourceCode": {}}"#;
        let type_not_specified_object_result =
            serde_json::from_str::<SourceCodeData>(type_not_specified_object_str);
        assert!(type_not_specified_object_result.is_err());
    }
}
