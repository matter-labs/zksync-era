pub mod storage_block;
use zksync_db_connection::error::SqlxContext;
use zksync_types::ProtocolVersionId;

mod call;
pub mod storage_base_token_ratio;
pub(crate) mod storage_data_availability;
pub mod storage_eth_tx;
pub mod storage_event;
pub mod storage_log;
pub mod storage_oracle_info;
pub mod storage_protocol_version;
pub mod storage_sync;
pub mod storage_tee_proof;
pub mod storage_transaction;
pub mod storage_verification_request;

#[cfg(test)]
mod tests;

pub(crate) fn parse_protocol_version(raw: i32) -> sqlx::Result<ProtocolVersionId> {
    u16::try_from(raw)
        .decode_column("protocol_version")?
        .try_into()
        .decode_column("protocol_version")
}
