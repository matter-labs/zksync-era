use crate::glue::{GlueFrom, GlueInto};

// Most of the types between 1.4.0 and 1.3.3 are shared and so are defined only once
impl GlueFrom<zk_evm_1_4_0::reference_impls::event_sink::EventMessage>
    for zksync_types::zk_evm_types::EventMessage
{
    fn glue_from(event: zk_evm_1_4_0::reference_impls::event_sink::EventMessage) -> Self {
        zksync_types::zk_evm_types::EventMessage {
            shard_id: event.shard_id,
            is_first: event.is_first,
            tx_number_in_block: event.tx_number_in_block,
            address: event.address,
            key: event.key,
            value: event.value,
        }
    }
}
