use zksync_utils::u256_to_h256;

use crate::glue::GlueFrom;

// Most of the types between `zk_evm@1.4.0` and `zk_evm@1.3.3` are shared and so we need only the additional conversion
// for `EventMessage` only.
impl GlueFrom<zk_evm_1_4_0::reference_impls::event_sink::EventMessage>
    for zksync_types::l2_to_l1_log::L2ToL1Log
{
    fn glue_from(event: zk_evm_1_4_0::reference_impls::event_sink::EventMessage) -> Self {
        Self {
            shard_id: event.shard_id,
            is_service: event.is_first,
            tx_number_in_block: event.tx_number_in_block,
            sender: event.address,
            key: u256_to_h256(event.key),
            value: u256_to_h256(event.value),
        }
    }
}
