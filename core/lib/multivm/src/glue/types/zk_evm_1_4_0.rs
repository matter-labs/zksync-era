use zk_evm_1_4_0::reference_impls::event_sink::EventMessage as EventMessage_1_4_0;
use zksync_types::l2_to_l1_log::L2ToL1Log;

use crate::glue::GlueFrom;

impl GlueFrom<EventMessage_1_4_0> for L2ToL1Log {
    fn glue_from(m: EventMessage_1_4_0) -> Self {
        Self {
            shard_id: m.shard_id,
            is_service: m.is_first,
            tx_number_in_block: m.tx_number_in_block,
            sender: m.address,
            key: zksync_utils::u256_to_h256(m.key),
            value: zksync_utils::u256_to_h256(m.value),
        }
    }
}
