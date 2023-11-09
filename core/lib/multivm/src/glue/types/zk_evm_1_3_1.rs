use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<zk_evm_1_3_1::aux_structures::Timestamp> for zksync_types::Timestamp {
    fn glue_from(timestamp: zk_evm_1_3_1::aux_structures::Timestamp) -> Self {
        zksync_types::Timestamp(timestamp.0)
    }
}

impl GlueFrom<zk_evm_1_3_1::aux_structures::LogQuery> for zksync_types::LogQuery {
    fn glue_from(query: zk_evm_1_3_1::aux_structures::LogQuery) -> Self {
        zksync_types::LogQuery {
            address: query.address,
            key: query.key,
            written_value: query.written_value,
            timestamp: query.timestamp.glue_into(),
            shard_id: query.shard_id,
            rollback: query.rollback,
            tx_number_in_block: query.tx_number_in_block,
            aux_byte: query.aux_byte,
            read_value: query.read_value,
            rw_flag: query.rw_flag,
            is_service: query.is_service,
        }
    }
}

impl GlueFrom<zksync_types::Timestamp> for zk_evm_1_3_1::aux_structures::Timestamp {
    fn glue_from(timestamp: zksync_types::Timestamp) -> Self {
        zk_evm_1_3_1::aux_structures::Timestamp(timestamp.0)
    }
}

impl GlueFrom<zksync_types::LogQuery> for zk_evm_1_3_1::aux_structures::LogQuery {
    fn glue_from(query: zksync_types::LogQuery) -> Self {
        zk_evm_1_3_1::aux_structures::LogQuery {
            address: query.address,
            key: query.key,
            written_value: query.written_value,
            timestamp: query.timestamp.glue_into(),
            shard_id: query.shard_id,
            rollback: query.rollback,
            tx_number_in_block: query.tx_number_in_block,
            aux_byte: query.aux_byte,
            read_value: query.read_value,
            rw_flag: query.rw_flag,
            is_service: query.is_service,
        }
    }
}

impl GlueFrom<zk_evm_1_3_1::reference_impls::event_sink::EventMessage>
    for zksync_types::EventMessage
{
    fn glue_from(event: zk_evm_1_3_1::reference_impls::event_sink::EventMessage) -> Self {
        zksync_types::EventMessage {
            shard_id: event.shard_id,
            is_first: event.is_first,
            tx_number_in_block: event.tx_number_in_block,
            address: event.address,
            key: event.key,
            value: event.value,
        }
    }
}

impl GlueFrom<zk_evm_1_3_1::zkevm_opcode_defs::FarCallOpcode> for zksync_types::FarCallOpcode {
    fn glue_from(value: zk_evm_1_3_1::zkevm_opcode_defs::FarCallOpcode) -> Self {
        match value {
            zk_evm_1_3_1::zkevm_opcode_defs::FarCallOpcode::Normal => Self::Normal,
            zk_evm_1_3_1::zkevm_opcode_defs::FarCallOpcode::Delegate => Self::Delegate,
            zk_evm_1_3_1::zkevm_opcode_defs::FarCallOpcode::Mimic => Self::Mimic,
        }
    }
}

impl GlueFrom<zksync_types::FarCallOpcode> for zk_evm_1_3_1::zkevm_opcode_defs::FarCallOpcode {
    fn glue_from(value: zksync_types::FarCallOpcode) -> Self {
        match value {
            zksync_types::FarCallOpcode::Normal => Self::Normal,
            zksync_types::FarCallOpcode::Delegate => Self::Delegate,
            zksync_types::FarCallOpcode::Mimic => Self::Mimic,
        }
    }
}
