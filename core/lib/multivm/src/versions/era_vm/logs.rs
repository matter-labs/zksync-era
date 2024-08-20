use zksync_types::l2_to_l1_log::{L2ToL1Log, SystemL2ToL1Log};
use zksync_utils::u256_to_h256;

pub trait IntoSystemLog {
    fn into_system_log(&self) -> SystemL2ToL1Log;
}

impl IntoSystemLog for era_vm::state::L2ToL1Log {
    fn into_system_log(&self) -> SystemL2ToL1Log {
        let era_vm::state::L2ToL1Log {
            key,
            value,
            is_service,
            address,
            shard_id,
            tx_number,
        } = *self;

        SystemL2ToL1Log(L2ToL1Log {
            shard_id,
            is_service,
            tx_number_in_block: tx_number,
            sender: address,
            key: u256_to_h256(key),
            value: u256_to_h256(value),
        })
    }
}
