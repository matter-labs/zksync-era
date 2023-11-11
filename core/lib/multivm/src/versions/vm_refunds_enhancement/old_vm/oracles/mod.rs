use zk_evm_1_3_3::aux_structures::Timestamp;

pub(crate) mod decommitter;
pub(crate) mod precompile;

pub(crate) trait OracleWithHistory {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp);
}
