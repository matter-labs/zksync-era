use zk_evm_1_3_3::{
    aux_structures::{LogQuery as LogQuery_1_3_3, Timestamp as Timestamp_1_3_3},
    zkevm_opcode_defs::FarCallOpcode as FarCallOpcode_1_3_3,
};
use zksync_types::zk_evm_types::{FarCallOpcode, LogQuery, Timestamp};
use zksync_utils::u256_to_h256;

use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<FarCallOpcode_1_3_3> for FarCallOpcode {
    fn glue_from(value: FarCallOpcode_1_3_3) -> Self {
        match value {
            FarCallOpcode_1_3_3::Normal => FarCallOpcode::Normal,
            FarCallOpcode_1_3_3::Delegate => FarCallOpcode::Delegate,
            FarCallOpcode_1_3_3::Mimic => FarCallOpcode::Mimic,
        }
    }
}

impl GlueFrom<zksync_types::zk_evm_types::FarCallOpcode>
    for zk_evm_1_3_3::zkevm_opcode_defs::FarCallOpcode
{
    fn glue_from(value: zksync_types::zk_evm_types::FarCallOpcode) -> Self {
        match value {
            zksync_types::zk_evm_types::FarCallOpcode::Normal => Self::Normal,
            zksync_types::zk_evm_types::FarCallOpcode::Delegate => Self::Delegate,
            zksync_types::zk_evm_types::FarCallOpcode::Mimic => Self::Mimic,
        }
    }
}

impl GlueFrom<Timestamp_1_3_3> for Timestamp {
    fn glue_from(value: Timestamp_1_3_3) -> Timestamp {
        Timestamp(value.0)
    }
}

impl GlueFrom<Timestamp> for Timestamp_1_3_3 {
    fn glue_from(value: Timestamp) -> Timestamp_1_3_3 {
        Timestamp_1_3_3(value.0)
    }
}

impl GlueFrom<LogQuery_1_3_3> for LogQuery {
    fn glue_from(value: LogQuery_1_3_3) -> LogQuery {
        LogQuery {
            timestamp: value.timestamp.glue_into(),
            tx_number_in_block: value.tx_number_in_block,
            aux_byte: value.aux_byte,
            shard_id: value.shard_id,
            address: value.address,
            key: value.key,
            read_value: value.read_value,
            written_value: value.written_value,
            rw_flag: value.rw_flag,
            rollback: value.rollback,
            is_service: value.is_service,
        }
    }
}

impl GlueFrom<LogQuery> for LogQuery_1_3_3 {
    fn glue_from(value: LogQuery) -> LogQuery_1_3_3 {
        LogQuery_1_3_3 {
            timestamp: value.timestamp.glue_into(),
            tx_number_in_block: value.tx_number_in_block,
            aux_byte: value.aux_byte,
            shard_id: value.shard_id,
            address: value.address,
            key: value.key,
            read_value: value.read_value,
            written_value: value.written_value,
            rw_flag: value.rw_flag,
            rollback: value.rollback,
            is_service: value.is_service,
        }
    }
}

impl GlueFrom<zk_evm_1_3_3::reference_impls::event_sink::EventMessage>
    for zksync_types::l2_to_l1_log::L2ToL1Log
{
    fn glue_from(event: zk_evm_1_3_3::reference_impls::event_sink::EventMessage) -> Self {
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
