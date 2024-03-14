use zksync_types::L1_GAS_PER_PUBDATA_BYTE;

pub trait PubdataPricing
where
    Self: std::fmt::Debug + Sync + Send,
{
    /// Returns the amount of L1 gas to publish a single byte of pub data.
    fn pubdata_byte_gas(&self) -> u64;
}

#[derive(Debug)]
pub struct ValidiumPubdataPricing;
#[derive(Debug)]
pub struct RollupPubdataPricing;

impl PubdataPricing for ValidiumPubdataPricing {
    fn pubdata_byte_gas(&self) -> u64 {
        0
    }
}

impl PubdataPricing for RollupPubdataPricing {
    fn pubdata_byte_gas(&self) -> u64 {
        L1_GAS_PER_PUBDATA_BYTE.into()
    }
}
