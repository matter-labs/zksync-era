pub mod storage_block;
use anyhow::Context as _;
use zksync_db_connection::error::SqlxContext;
use zksync_types::{ProtocolVersionId, H160, H256};

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
pub mod storage_witness_job_info;
#[cfg(test)]
mod tests;

pub(crate) fn parse_h256(bytes: &[u8]) -> anyhow::Result<H256> {
    Ok(<[u8; 32]>::try_from(bytes).context("invalid size")?.into())
}

fn parse_h256_opt(bytes: Option<&[u8]>) -> anyhow::Result<H256> {
    parse_h256(bytes.context("missing data")?)
}

pub(crate) fn parse_h160(bytes: &[u8]) -> anyhow::Result<H160> {
    Ok(<[u8; 20]>::try_from(bytes).context("invalid size")?.into())
}

pub(crate) fn parse_protocol_version(raw: i32) -> sqlx::Result<ProtocolVersionId> {
    u16::try_from(raw)
        .decode_column("protocol_version")?
        .try_into()
        .decode_column("protocol_version")
}
