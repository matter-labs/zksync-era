pub mod consensus;
mod proto;
pub mod storage_block;
pub mod storage_eth_tx;
pub mod storage_event;
pub mod storage_fee_monitor;
pub mod storage_log;
pub mod storage_protocol_version;
pub mod storage_sync;
pub mod storage_transaction;
pub mod storage_verification_request;
pub mod storage_witness_job_info;
#[cfg(test)]
mod tests;

use anyhow::Context;
use zksync_types::{H160, H256};

fn parse_h256(bytes: &[u8]) -> anyhow::Result<H256> {
    Ok(<[u8; 32]>::try_from(bytes).context("invalid size")?.into())
}

fn parse_h160(bytes: &[u8]) -> anyhow::Result<H160> {
    Ok(<[u8; 20]>::try_from(bytes).context("invalid size")?.into())
}
