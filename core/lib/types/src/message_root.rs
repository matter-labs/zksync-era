use crate::U256;

#[derive(Debug, Clone)]
pub struct MessageRoot {
    pub chain_id: u32,
    pub block_number: u32,
    pub sides: Vec<U256>, // The rolling hash of all the transactions in the miniblock
}

impl MessageRoot {
    pub fn new(chain_id: u32, block_number: u32, sides: Vec<U256>) -> Self {
        Self {
            chain_id,
            block_number,
            sides,
        }
    }
}
