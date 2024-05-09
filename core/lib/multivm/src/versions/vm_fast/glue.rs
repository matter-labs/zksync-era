use zksync_types::{
    l2_to_l1_log::{L2ToL1Log, SystemL2ToL1Log},
    zk_evm_types::{LogQuery, Timestamp},
    StorageLogQuery, H160, U256,
};
use zksync_utils::u256_to_h256;

use crate::glue::GlueFrom;

impl GlueFrom<&vm2::L2ToL1Log> for SystemL2ToL1Log {
    fn glue_from(value: &vm2::L2ToL1Log) -> Self {
        let vm2::L2ToL1Log {
            key,
            value,
            is_service,
            address,
            shard_id,
            tx_number,
        } = *value;

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

pub(crate) fn storage_log_query_from_change(
    ((address, key), (before, after)): ((H160, U256), (Option<U256>, U256)),
) -> StorageLogQuery {
    StorageLogQuery {
        log_query: LogQuery {
            timestamp: Timestamp(0),
            tx_number_in_block: 0, // incorrect and hopefully unused
            aux_byte: 0,           // incorrect and hopefully unused
            shard_id: 0,
            address,
            key,
            read_value: before.unwrap_or_default(),
            written_value: after,
            rw_flag: true,
            rollback: false,
            is_service: false, // incorrect and hopefully unused
        },
        log_type: zksync_types::StorageLogQueryType::RepeatedWrite, // incorrect and hopefully unused
    }
}
