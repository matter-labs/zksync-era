use zksync_config::configs::{self};
use zksync_protobuf::ProtoRepr;

use crate::proto::use_evm_simulator as proto;

impl ProtoRepr for proto::UseEvmSimulator {
    type Type = configs::use_evm_simulator::UseEvmSimulator;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(configs::use_evm_simulator::UseEvmSimulator {
            use_evm_simulator: self.use_evm_simulator.unwrap_or_default(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            use_evm_simulator: Some(this.use_evm_simulator),
        }
    }
}
