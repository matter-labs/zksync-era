use zksync_types::L1_GAS_PER_PUBDATA_BYTE;

use crate::state_keeper::metrics::KEEPER_METRICS;

pub trait PubdataPricing
where
    Self: std::fmt::Debug + Sync + Send,
{
    /// Returns the amount of L1 gas to publish a single byte of pub data in PubdataSendingMode::Calldata.
    fn pubdata_byte_gas(&self) -> u64;
    /// Returns the amount of L1 gas to publish a single byte of pub data in PubdataSendingMode::Blobs.
    fn bound_blob_base_fee(&self, blob_base_fee: f64, max_blob_base_fee: u64) -> u64;
}

#[derive(Debug)]
pub struct ValidiumPubdataPricing;
#[derive(Debug)]
pub struct RollupPubdataPricing;

impl PubdataPricing for ValidiumPubdataPricing {
    fn pubdata_byte_gas(&self) -> u64 {
        0
    }

    fn bound_blob_base_fee(&self, _blob_base_fee: f64, _max_blob_base_fee: u64) -> u64 {
        0
    }
}

impl PubdataPricing for RollupPubdataPricing {
    fn pubdata_byte_gas(&self) -> u64 {
        L1_GAS_PER_PUBDATA_BYTE.into()
    }

    fn bound_blob_base_fee(&self, blob_base_fee: f64, max_blob_base_fee: u64) -> u64 {
        if blob_base_fee > max_blob_base_fee as f64 {
            tracing::error!("Blob base fee is too high: {blob_base_fee}, using max allowed: {max_blob_base_fee}");
            KEEPER_METRICS.gas_price_too_high.inc();
            return max_blob_base_fee;
        }
        blob_base_fee as u64
    }
}
