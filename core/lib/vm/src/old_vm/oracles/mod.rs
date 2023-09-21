use zk_evm::aux_structures::Timestamp;

pub(crate) mod decommitter;
pub(crate) mod precompile;
pub(crate) mod storage;

pub(crate) trait OracleWithHistory {
    fn rollback_to_timestamp(&mut self, timestamp: Timestamp);
}
