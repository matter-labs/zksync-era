use std::{fmt::Debug, str::FromStr};

use serde::{Deserialize, Serialize};
use zksync_contracts::BaseSystemContracts;
use zksync_types::{
    commitment::{L1BatchCommitmentMode, PubdataParams},
    Address, L2ChainId, ProtocolVersionId,
};

// #[derive(Copy, Debug, Clone, PartialEq, Serialize, Deserialize)]
// pub enum PubdataType {
//     Rollup,
//     Validium,
// }

// impl From<L1BatchCommitmentMode> for PubdataType {
//     fn from(mode: L1BatchCommitmentMode) -> Self {
//         match mode {
//             L1BatchCommitmentMode::Rollup => Self::Rollup,
//             L1BatchCommitmentMode::Validium => Self::Validium,
//         }
//     }
// }

// #[derive(Copy, Debug, Clone, PartialEq, Serialize, Deserialize)]
// pub struct PubdataParams {
//     pub l2_da_validator_address: Address,
//     pub pubdata_type: PubdataType,
// }

// impl PubdataParams {
//     /// FIXME: this is not how it should be in production, but we do it this way for quicker testing.
//     pub fn extract_from_env() -> Self {
//         let l1_batch_commit_data_generator_mode_str =
//             std::env::var("CHAIN_STATE_KEEPER_L1_BATCH_COMMIT_DATA_GENERATOR_MODE")
//                 .unwrap_or_else(|_| "Rollup".to_string());
//         let l2_da_validator_str = std::env::var("CONTRACTS_L2_DA_VALIDATOR_ADDR").unwrap();

//         let pubdata_type = if l1_batch_commit_data_generator_mode_str == "Rollup" {
//             PubdataType::Rollup
//         } else if l1_batch_commit_data_generator_mode_str == "Validium" {
//             PubdataType::Validium
//         } else {
//             panic!(
//                 "Unsupported pubdata type: {}",
//                 l1_batch_commit_data_generator_mode_str
//             )
//         };

//         let l2_da_validator_address = Address::from_str(&l2_da_validator_str)
//             .expect("Failed to parse L2 DA validator address");

//         Self {
//             l2_da_validator_address,
//             pubdata_type,
//         }
//     }
// }

// impl Default for PubdataParams {
//     fn default() -> Self {
//         Self {
//             l2_da_validator_address: Address::default(),
//             pubdata_type: PubdataType::Rollup,
//         }
//     }
// }

/// Params related to the execution process, not batch it self
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemEnv {
    // Always false for VM
    pub zk_porter_available: bool,
    pub version: ProtocolVersionId,
    pub base_system_smart_contracts: BaseSystemContracts,
    pub bootloader_gas_limit: u32,
    pub execution_mode: TxExecutionMode,
    pub default_validation_computational_gas_limit: u32,
    pub chain_id: L2ChainId,
    pub pubdata_params: PubdataParams,
}

impl Debug for SystemEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemEnv")
            .field("zk_porter_available", &self.zk_porter_available)
            .field("version", &self.version)
            .field(
                "base_system_smart_contracts",
                &self.base_system_smart_contracts.hashes(),
            )
            .field("gas_limit", &self.bootloader_gas_limit)
            .field(
                "default_validation_computational_gas_limit",
                &self.default_validation_computational_gas_limit,
            )
            .field("execution_mode", &self.execution_mode)
            .field("chain_id", &self.chain_id)
            .field("pubdata_params", &self.pubdata_params)
            .finish()
    }
}

/// Enum denoting the *in-server* execution mode for the bootloader transactions.
///
/// If `EthCall` mode is chosen, the bootloader will use `mimicCall` opcode
/// to simulate the call instead of using the standard `execute` method of account.
/// This is needed to be able to behave equivalently to Ethereum without much overhead for custom account builders.
/// With `VerifyExecute` mode, transaction will be executed normally.
/// With `EstimateFee`, the bootloader will be used that has the same behavior
/// as the full `VerifyExecute` block, but errors in the account validation will be ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxExecutionMode {
    VerifyExecute,
    EstimateFee,
    EthCall,
}
