use crate::{CircuitBreaker, CircuitBreakerError};
use thiserror::Error;
use zksync_config::ZkSyncConfig;
use zksync_contracts::{DEFAULT_ACCOUNT_CODE, PROVED_BLOCK_BOOTLOADER_CODE};
use zksync_eth_client::clients::http_client::EthereumClient;
use zksync_types::H256;
use zksync_utils::u256_to_h256;

#[derive(Debug, Error)]
pub enum MismatchedCodeHashError {
    #[error("Server has different bootloader code hash from the one on L1 contract, server: {server_hash:?}, contract: {contract_hash:?}")]
    Bootloader {
        server_hash: H256,
        contract_hash: H256,
    },
    #[error("Server has different default account code hash from the one on L1 contract, server: {server_hash:?}, contract: {contract_hash:?}")]
    DefaultAccount {
        server_hash: H256,
        contract_hash: H256,
    },
}

#[derive(Debug)]
pub struct CodeHashesChecker {
    pub eth_client: EthereumClient,
}

impl CodeHashesChecker {
    pub fn new(config: &ZkSyncConfig) -> Self {
        Self {
            eth_client: EthereumClient::from_config(config),
        }
    }
}

#[async_trait::async_trait]
impl CircuitBreaker for CodeHashesChecker {
    async fn check(&self) -> Result<(), CircuitBreakerError> {
        let bootloader_code_hash_on_l1: H256 = self
            .eth_client
            .call_main_contract_function(
                "getL2BootloaderBytecodeHash",
                (),
                None,
                Default::default(),
                None,
            )
            .await
            .unwrap();
        if bootloader_code_hash_on_l1 != u256_to_h256(PROVED_BLOCK_BOOTLOADER_CODE.hash) {
            return Err(CircuitBreakerError::MismatchedCodeHash(
                MismatchedCodeHashError::Bootloader {
                    server_hash: u256_to_h256(PROVED_BLOCK_BOOTLOADER_CODE.hash),
                    contract_hash: bootloader_code_hash_on_l1,
                },
            ));
        }

        let default_account_code_hash_on_l1: H256 = self
            .eth_client
            .call_main_contract_function(
                "getL2DefaultAccountBytecodeHash",
                (),
                None,
                Default::default(),
                None,
            )
            .await
            .unwrap();
        if default_account_code_hash_on_l1 != u256_to_h256(DEFAULT_ACCOUNT_CODE.hash) {
            return Err(CircuitBreakerError::MismatchedCodeHash(
                MismatchedCodeHashError::DefaultAccount {
                    server_hash: u256_to_h256(DEFAULT_ACCOUNT_CODE.hash),
                    contract_hash: default_account_code_hash_on_l1,
                },
            ));
        }
        Ok(())
    }
}
