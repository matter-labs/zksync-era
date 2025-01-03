
use zksync_types::H256;

#[derive(Debug)]
pub(crate) struct MessageRoot {
    pub(crate) chain_id: u32,
    pub(crate) block_number: u32,
    pub(crate) hash: H256, // The rolling hash of all the transactions in the miniblock
}

impl MessageRoot {
    pub(crate) fn new(chain_id: u32, block_number: u32, hash: H256) -> Self {
        Self {
            chain_id,
            block_number,
            hash,
        }
    }
}