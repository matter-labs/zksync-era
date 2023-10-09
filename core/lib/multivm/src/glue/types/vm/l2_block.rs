use crate::glue::GlueFrom;

impl GlueFrom<vm_latest::L2BlockEnv> for vm_virtual_blocks::L2BlockEnv {
    fn glue_from(value: vm_latest::L2BlockEnv) -> Self {
        Self {
            number: value.number,
            timestamp: value.timestamp,
            prev_block_hash: value.prev_block_hash,
            max_virtual_blocks_to_create: value.max_virtual_blocks_to_create,
        }
    }
}
