use std::fmt;

use anyhow::Result;
use async_trait::async_trait;
use zksync_types::U256;

#[async_trait]
pub trait InteropFeeInputProvider: fmt::Debug + Send + Sync + 'static {
    async fn get_interop_fee(&self) -> Result<U256>;
}

#[derive(Debug, Clone)]
pub struct ConstantInteropFeeInputProvider {
    fee: U256,
}

impl ConstantInteropFeeInputProvider {
    pub fn new(fee: U256) -> Self {
        Self { fee }
    }
}

#[async_trait]
impl InteropFeeInputProvider for ConstantInteropFeeInputProvider {
    async fn get_interop_fee(&self) -> Result<U256> {
        Ok(self.fee)
    }
}
