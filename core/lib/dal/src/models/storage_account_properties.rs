use bigdecimal::{BigDecimal, ToPrimitive};
use zksync_types::{boojum_os::AccountProperties, H256};

use crate::models::bigdecimal_to_u256;

#[derive(Clone, Debug)]
pub struct StorageAccountProperties {
    pub versioning_data: BigDecimal,
    pub nonce: BigDecimal,
    pub observable_bytecode_hash: Vec<u8>,
    pub bytecode_hash: Vec<u8>,
    // TODO: rename to balance?
    pub nominal_token_balance: BigDecimal,
    pub bytecode_len: i64,
    pub artifacts_len: i64,
    pub observable_bytecode_len: i64,
}

impl From<StorageAccountProperties> for AccountProperties {
    fn from(value: StorageAccountProperties) -> Self {
        AccountProperties {
            versioning_data: value.versioning_data.to_u64().unwrap(),
            nonce: value.nonce.to_u64().unwrap(),
            observable_bytecode_hash: H256::from_slice(&value.observable_bytecode_hash),
            bytecode_hash: H256::from_slice(&value.bytecode_hash),
            nominal_token_balance: bigdecimal_to_u256(value.nominal_token_balance),
            bytecode_len: u32::try_from(value.bytecode_len).unwrap(),
            artifacts_len: u32::try_from(value.artifacts_len).unwrap(),
            observable_bytecode_len: u32::try_from(value.observable_bytecode_len).unwrap(),
        }
    }
}
