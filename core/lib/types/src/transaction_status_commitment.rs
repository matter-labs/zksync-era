use crate::H256;

#[derive(Debug, Clone)]
pub struct TransactionStatusCommitment {
    pub tx_hash: H256,
    pub is_success: bool,
}

const PACKED_BYTES_SIZE: usize = 33;

impl TransactionStatusCommitment {
    pub fn get_packed_bytes(&self) -> [u8; PACKED_BYTES_SIZE] {
        let status_byte = if self.is_success { 1u8 } else { 0u8 };

        let mut data = [0u8; PACKED_BYTES_SIZE];
        data[0..32].copy_from_slice(self.tx_hash.as_bytes());
        data[32] = status_byte;

        data
    }
}
