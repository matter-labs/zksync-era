use std::fmt;

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait InteropFeeInputProvider: fmt::Debug + Send + Sync + 'static {
    async fn get_interop_fee(&self) -> Result<u64>;
}

#[derive(Debug, Clone)]
pub struct ConstantInteropFeeInputProvider {
    fee: u64,
}

impl ConstantInteropFeeInputProvider {
    pub fn new(fee: u64) -> Self {
        Self { fee }
    }
}

#[async_trait]
impl InteropFeeInputProvider for ConstantInteropFeeInputProvider {
    async fn get_interop_fee(&self) -> Result<u64> {
        Ok(self.fee)
    }
}
