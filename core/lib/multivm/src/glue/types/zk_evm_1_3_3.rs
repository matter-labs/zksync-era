use zk_evm_1_3_3::{
    aux_structures::{LogQuery as LogQuery_1_3_3, Timestamp as Timestamp_1_3_3},
    zkevm_opcode_defs::FarCallOpcode as FarCallOpcode_1_3_3,
};
use zksync_types::zk_evm_types::{FarCallOpcode, LogQuery, Timestamp};

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
