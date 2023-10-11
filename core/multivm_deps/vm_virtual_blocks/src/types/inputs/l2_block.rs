use zksync_types::H256;

#[derive(Debug, Clone, Copy)]
pub struct L2BlockEnv {
    pub number: u32,
    pub timestamp: u64,
    pub prev_block_hash: H256,
    pub max_virtual_blocks_to_create: u32,
}
