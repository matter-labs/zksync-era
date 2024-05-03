use zk_evm_1_4_1::{
    aux_structures::{LogQuery as LogQuery_1_4_1, Timestamp as Timestamp_1_4_1},
    zkevm_opcode_defs::FarCallOpcode as FarCallOpcode_1_4_1,
};
use zksync_types::zk_evm_types::{FarCallOpcode, LogQuery, Timestamp};
use zksync_utils::u256_to_h256;

use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<FarCallOpcode_1_4_1> for FarCallOpcode {
    fn glue_from(value: FarCallOpcode_1_4_1) -> Self {
        match value {
            FarCallOpcode_1_4_1::Normal => FarCallOpcode::Normal,
            FarCallOpcode_1_4_1::Delegate => FarCallOpcode::Delegate,
            FarCallOpcode_1_4_1::Mimic => FarCallOpcode::Mimic,
        }
    }
}

impl GlueFrom<Timestamp_1_4_1> for Timestamp {
    fn glue_from(value: Timestamp_1_4_1) -> Timestamp {
        Timestamp(value.0)
    }
}

impl GlueFrom<Timestamp> for Timestamp_1_4_1 {
    fn glue_from(value: Timestamp) -> Timestamp_1_4_1 {
        Timestamp_1_4_1(value.0)
    }
}

impl GlueFrom<LogQuery_1_4_1> for LogQuery {
    fn glue_from(value: LogQuery_1_4_1) -> LogQuery {
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

impl GlueFrom<LogQuery> for LogQuery_1_4_1 {
    fn glue_from(value: LogQuery) -> LogQuery_1_4_1 {
        LogQuery_1_4_1 {
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

impl GlueFrom<zk_evm_1_4_1::reference_impls::event_sink::EventMessage>
    for zksync_types::l2_to_l1_log::L2ToL1Log
{
    fn glue_from(event: zk_evm_1_4_1::reference_impls::event_sink::EventMessage) -> Self {
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
