use zksync_types::{explorer_api::ContractBasicInfo, Address, Bytes, MiniblockNumber, H256};
use zksync_utils::h256_to_account_address;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageContractInfo {
    pub key_address: Vec<u8>,
    pub bytecode: Vec<u8>,
    pub creator_address: Option<Vec<u8>>,
    pub creator_tx_hash: Option<Vec<u8>>,
    pub created_in_block_number: i64,
    pub verification_info: Option<serde_json::Value>,
}

impl From<StorageContractInfo> for ContractBasicInfo {
    fn from(info: StorageContractInfo) -> ContractBasicInfo {
        ContractBasicInfo {
            address: h256_to_account_address(&H256::from_slice(&info.key_address)),
            bytecode: Bytes(info.bytecode),
            creator_address: info
                .creator_address
                .map(|address| Address::from_slice(&address))
                .unwrap_or_else(Address::zero),
            creator_tx_hash: info
                .creator_tx_hash
                .map(|tx_hash| H256::from_slice(&tx_hash))
                .unwrap_or_else(H256::zero),
            created_in_block_number: MiniblockNumber(info.created_in_block_number as u32),
            verification_info: info.verification_info.map(|verification_info| {
                serde_json::from_value(verification_info)
                    .expect("invalid verification_info json in database")
            }),
        }
    }
}
