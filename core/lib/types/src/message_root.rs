use crate::H256;

#[derive(Debug, Clone, Copy)]
pub struct MessageRoot {
    pub chain_id: u32,
    pub block_number: u32,
    pub hash: H256, // The rolling hash of all the transactions in the miniblock
}

impl MessageRoot {
    pub fn new(chain_id: u32, block_number: u32, hash: H256) -> Self {
        Self {
            chain_id,
            block_number,
            hash,
        }
    }
}
