use zksync_types::l2_to_l1_log::{L2ToL1Log, SystemL2ToL1Log};
use zksync_utils::u256_to_h256;
use zksync_vm2::interface;

use crate::glue::GlueFrom;

impl GlueFrom<interface::L2ToL1Log> for SystemL2ToL1Log {
    fn glue_from(value: interface::L2ToL1Log) -> Self {
        let interface::L2ToL1Log {
            key,
            value,
            is_service,
            address,
            shard_id,
            tx_number,
        } = value;

        Self(L2ToL1Log {
            shard_id,
            is_service,
            tx_number_in_block: tx_number,
            sender: address,
            key: u256_to_h256(key),
            value: u256_to_h256(value),
        })
    }
}
