use zksync_types::u256_to_h256;

use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<zk_evm_1_5_2::aux_structures::Timestamp> for zksync_types::zk_evm_types::Timestamp {
    fn glue_from(timestamp: zk_evm_1_5_2::aux_structures::Timestamp) -> Self {
        zksync_types::zk_evm_types::Timestamp(timestamp.0)
    }
}

impl GlueFrom<zk_evm_1_5_2::aux_structures::LogQuery> for zksync_types::zk_evm_types::LogQuery {
    fn glue_from(query: zk_evm_1_5_2::aux_structures::LogQuery) -> Self {
        zksync_types::zk_evm_types::LogQuery {
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

impl GlueFrom<zksync_types::zk_evm_types::Timestamp> for zk_evm_1_5_2::aux_structures::Timestamp {
    fn glue_from(timestamp: zksync_types::zk_evm_types::Timestamp) -> Self {
        zk_evm_1_5_2::aux_structures::Timestamp(timestamp.0)
    }
}

impl GlueFrom<zksync_types::zk_evm_types::LogQuery> for zk_evm_1_5_2::aux_structures::LogQuery {
    fn glue_from(query: zksync_types::zk_evm_types::LogQuery) -> Self {
        zk_evm_1_5_2::aux_structures::LogQuery {
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

impl GlueFrom<zk_evm_1_5_2::zkevm_opcode_defs::FarCallOpcode>
    for zksync_types::zk_evm_types::FarCallOpcode
{
    fn glue_from(value: zk_evm_1_5_2::zkevm_opcode_defs::FarCallOpcode) -> Self {
        match value {
            zk_evm_1_5_2::zkevm_opcode_defs::FarCallOpcode::Normal => Self::Normal,
            zk_evm_1_5_2::zkevm_opcode_defs::FarCallOpcode::Delegate => Self::Delegate,
            zk_evm_1_5_2::zkevm_opcode_defs::FarCallOpcode::Mimic => Self::Mimic,
        }
    }
}

impl GlueFrom<zksync_types::zk_evm_types::FarCallOpcode>
    for zk_evm_1_5_2::zkevm_opcode_defs::FarCallOpcode
{
    fn glue_from(value: zksync_types::zk_evm_types::FarCallOpcode) -> Self {
        match value {
            zksync_types::zk_evm_types::FarCallOpcode::Normal => Self::Normal,
            zksync_types::zk_evm_types::FarCallOpcode::Delegate => Self::Delegate,
            zksync_types::zk_evm_types::FarCallOpcode::Mimic => Self::Mimic,
        }
    }
}

impl GlueFrom<zk_evm_1_5_2::reference_impls::event_sink::EventMessage>
    for zksync_types::l2_to_l1_log::L2ToL1Log
{
    fn glue_from(event: zk_evm_1_5_2::reference_impls::event_sink::EventMessage) -> Self {
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
