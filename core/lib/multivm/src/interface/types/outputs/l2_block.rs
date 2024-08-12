use zksync_types::H256;

#[derive(Debug)]
pub struct L2Block {
    pub number: u32,
    pub timestamp: u64,
    pub hash: H256,
}
