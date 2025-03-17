use std::convert::TryInto;

use zksync_types::{
    address_to_h256,
    bytecode::BytecodeHash,
    ethabi::{encode, Address, Token},
    fee::{encoding_len, Fee},
    h256_to_u256,
    l1::is_l1_tx_type,
    l2::{L2Tx, TransactionType},
    transaction_request::{PaymasterParams, TransactionRequest},
    web3::Bytes,
    Execute, ExecuteTransactionCommon, L2ChainId, L2TxCommonData, Nonce, Transaction, H256, U256,
};

use crate::{
    utils::bytecode::bytes_to_be_words,
    vm_latest::{
        constants::{MAX_GAS_PER_PUBDATA_BYTE, TX_MAX_COMPUTE_GAS_LIMIT},
        utils::overhead::derive_overhead,
    },
};

/// This structure represents the data that is used by
/// the Bootloader to describe the transaction.
#[derive(Debug, Default, Clone)]
pub(crate) struct TransactionData {
    pub(crate) tx_type: u8,
    pub(crate) from: Address,
    pub(crate) to: Option<Address>,
    pub(crate) gas_limit: U256,
    pub(crate) pubdata_price_limit: U256,
    pub(crate) max_fee_per_gas: U256,
    pub(crate) max_priority_fee_per_gas: U256,
    pub(crate) paymaster: Address,
    pub(crate) nonce: U256,
    pub(crate) value: U256,
    // The reserved fields that are unique for different types of transactions.
    // E.g. nonce is currently used in all transaction, but it should not be mandatory
    // in the long run.
    pub(crate) reserved: [U256; 4],
    pub(crate) data: Vec<u8>,
    pub(crate) signature: Vec<u8>,
    // The factory deps provided with the transaction.
    // Note that *only hashes* of these bytecodes are signed by the user
    // and they are used in the ABI encoding of the struct.
    // TODO: include this into the tx signature as part of SMA-1010
    pub(crate) factory_deps: Vec<Vec<u8>>,
    pub(crate) paymaster_input: Vec<u8>,
    pub(crate) reserved_dynamic: Vec<u8>,
    pub(crate) raw_bytes: Option<Vec<u8>>,
}

impl TransactionData {
    pub(crate) fn new(execute_tx: Transaction, use_evm_emulator: bool) -> Self {
        match execute_tx.common_data {
            ExecuteTransactionCommon::L2(common_data) => {
                let nonce = U256::from_big_endian(&common_data.nonce.to_be_bytes());

                let should_check_chain_id = if matches!(
                    common_data.transaction_type,
                    TransactionType::LegacyTransaction
                ) && common_data.extract_chain_id().is_some()
                {
                    U256([1, 0, 0, 0])
                } else {
                    U256::zero()
                };

                let should_deploy_contract = if execute_tx.execute.contract_address.is_none() {
                    // Transactions with no `contract_address` should be filtered out by the API server,
                    // so this is more of a sanity check.
                    assert!(
                        use_evm_emulator,
                        "`execute.contract_address` not set for transaction {:?} with EVM emulation disabled",
                        common_data.hash()
                    );
                    U256([1, 0, 0, 0])
                } else {
                    U256::zero()
                };

                // Ethereum transactions do not sign gas per pubdata limit, and so for them we need to use
                // some default value. We use the maximum possible value that is allowed by the bootloader
                // (i.e. we can not use u64::MAX, because the bootloader requires gas per pubdata for such
                // transactions to be higher than `MAX_GAS_PER_PUBDATA_BYTE`).
                let gas_per_pubdata_limit = if common_data.transaction_type.is_ethereum_type() {
                    MAX_GAS_PER_PUBDATA_BYTE.into()
                } else {
                    common_data.fee.gas_per_pubdata_limit
                };

                TransactionData {
                    tx_type: (common_data.transaction_type as u32) as u8,
                    from: common_data.initiator_address,
                    to: execute_tx.execute.contract_address,
                    gas_limit: common_data.fee.gas_limit,
                    pubdata_price_limit: gas_per_pubdata_limit,
                    max_fee_per_gas: common_data.fee.max_fee_per_gas,
                    max_priority_fee_per_gas: common_data.fee.max_priority_fee_per_gas,
                    paymaster: common_data.paymaster_params.paymaster,
                    nonce,
                    value: execute_tx.execute.value,
                    reserved: [
                        should_check_chain_id,
                        should_deploy_contract,
                        U256::zero(),
                        U256::zero(),
                    ],
                    data: execute_tx.execute.calldata,
                    signature: common_data.signature,
                    factory_deps: execute_tx.execute.factory_deps,
                    paymaster_input: common_data.paymaster_params.paymaster_input,
                    reserved_dynamic: vec![],
                    raw_bytes: execute_tx.raw_bytes.map(|a| a.0),
                }
            }
            ExecuteTransactionCommon::L1(common_data) => {
                let refund_recipient = h256_to_u256(address_to_h256(&common_data.refund_recipient));
                TransactionData {
                    tx_type: common_data.tx_format() as u8,
                    from: common_data.sender,
                    to: execute_tx.execute.contract_address,
                    gas_limit: common_data.gas_limit,
                    pubdata_price_limit: common_data.gas_per_pubdata_limit,
                    // It doesn't matter what we put here, since
                    // the bootloader does not charge anything
                    max_fee_per_gas: common_data.max_fee_per_gas,
                    max_priority_fee_per_gas: U256::zero(),
                    paymaster: Address::default(),
                    nonce: U256::from(common_data.serial_id.0), // priority op ID
                    value: execute_tx.execute.value,
                    reserved: [
                        common_data.to_mint,
                        refund_recipient,
                        U256::zero(),
                        U256::zero(),
                    ],
                    data: execute_tx.execute.calldata,
                    // The signature isn't checked for L1 transactions so we don't care
                    signature: vec![],
                    factory_deps: execute_tx.execute.factory_deps,
                    paymaster_input: vec![],
                    reserved_dynamic: vec![],
                    raw_bytes: None,
                }
            }
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                let refund_recipient = h256_to_u256(address_to_h256(&common_data.refund_recipient));
                TransactionData {
                    tx_type: common_data.tx_format() as u8,
                    from: common_data.sender,
                    to: execute_tx.execute.contract_address,
                    gas_limit: common_data.gas_limit,
                    pubdata_price_limit: common_data.gas_per_pubdata_limit,
                    // It doesn't matter what we put here, since
                    // the bootloader does not charge anything
                    max_fee_per_gas: common_data.max_fee_per_gas,
                    max_priority_fee_per_gas: U256::zero(),
                    paymaster: Address::default(),
                    nonce: U256::from(common_data.upgrade_id as u16),
                    value: execute_tx.execute.value,
                    reserved: [
                        common_data.to_mint,
                        refund_recipient,
                        U256::zero(),
                        U256::zero(),
                    ],
                    data: execute_tx.execute.calldata,
                    // The signature isn't checked for L1 transactions so we don't care
                    signature: vec![],
                    factory_deps: execute_tx.execute.factory_deps,
                    paymaster_input: vec![],
                    reserved_dynamic: vec![],
                    raw_bytes: None,
                }
            }
        }
    }
}

impl TransactionData {
    pub(crate) fn abi_encode_with_custom_factory_deps(
        self,
        factory_deps_hashes: Vec<U256>,
    ) -> Vec<u8> {
        encode(&[Token::Tuple(vec![
            Token::Uint(U256::from_big_endian(&self.tx_type.to_be_bytes())),
            Token::Address(self.from),
            Token::Address(self.to.unwrap_or_default()),
            Token::Uint(self.gas_limit),
            Token::Uint(self.pubdata_price_limit),
            Token::Uint(self.max_fee_per_gas),
            Token::Uint(self.max_priority_fee_per_gas),
            Token::Address(self.paymaster),
            Token::Uint(self.nonce),
            Token::Uint(self.value),
            Token::FixedArray(self.reserved.iter().copied().map(Token::Uint).collect()),
            Token::Bytes(self.data),
            Token::Bytes(self.signature),
            Token::Array(factory_deps_hashes.into_iter().map(Token::Uint).collect()),
            Token::Bytes(self.paymaster_input),
            Token::Bytes(self.reserved_dynamic),
        ])])
    }

    pub(crate) fn abi_encode(self) -> Vec<u8> {
        let factory_deps_hashes = self
            .factory_deps
            .iter()
            .map(|dep| BytecodeHash::for_bytecode(dep).value_u256())
            .collect();
        self.abi_encode_with_custom_factory_deps(factory_deps_hashes)
    }

    pub(crate) fn into_tokens(self) -> Vec<U256> {
        bytes_to_be_words(&self.abi_encode())
    }

    pub(crate) fn overhead_gas(&self) -> u32 {
        let encoded_len = encoding_len(
            self.data.len() as u64,
            self.signature.len() as u64,
            self.factory_deps.len() as u64,
            self.paymaster_input.len() as u64,
            self.reserved_dynamic.len() as u64,
        );

        derive_overhead(encoded_len)
    }

    pub(crate) fn trusted_ergs_limit(&self) -> U256 {
        // No transaction is allowed to spend more than `TX_MAX_COMPUTE_GAS_LIMIT` gas on compute.
        U256::from(TX_MAX_COMPUTE_GAS_LIMIT).min(self.gas_limit)
    }

    pub(crate) fn signed_and_tx_hashes(&self, chain_id: L2ChainId) -> (Option<H256>, H256) {
        if is_l1_tx_type(self.tx_type) {
            return (None, self.canonical_l1_tx_hash());
        }

        let l2_tx: L2Tx = self.clone().try_into().unwrap();
        let mut transaction_request: TransactionRequest = l2_tx.into();
        transaction_request.chain_id = Some(chain_id.as_u64());

        // It is assumed that the `TransactionData` always has all the necessary components to recover the hash.
        let (signed_hash, tx_hash) = transaction_request
            .get_signed_and_tx_hashes()
            .expect("Could not recover L2 transaction hash");
        (Some(signed_hash), tx_hash)
    }

    fn canonical_l1_tx_hash(&self) -> H256 {
        use zksync_types::web3::keccak256;

        let encoded_bytes = self.clone().abi_encode();
        H256(keccak256(&encoded_bytes))
    }

    pub(crate) fn parse_signature(&self) -> Option<(bool, &[u8])> {
        if self.signature.len() != 65 {
            return None;
        }
        let v = match self.signature[64] {
            27 => false,
            28 => true,
            _ => return None,
        };
        Some((v, &self.signature[..64]))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TxHashCalculationError {
    CannotCalculateL2HashForL1Tx,
}

impl TryInto<L2Tx> for TransactionData {
    type Error = TxHashCalculationError;

    fn try_into(self) -> Result<L2Tx, Self::Error> {
        if is_l1_tx_type(self.tx_type) {
            return Err(TxHashCalculationError::CannotCalculateL2HashForL1Tx);
        }

        let common_data = L2TxCommonData {
            transaction_type: (self.tx_type as u32).try_into().unwrap(),
            nonce: Nonce(self.nonce.as_u32()),
            fee: Fee {
                max_fee_per_gas: self.max_fee_per_gas,
                max_priority_fee_per_gas: self.max_priority_fee_per_gas,
                gas_limit: self.gas_limit,
                gas_per_pubdata_limit: self.pubdata_price_limit,
            },
            signature: self.signature,
            input: None,
            initiator_address: self.from,
            paymaster_params: PaymasterParams {
                paymaster: self.paymaster,
                paymaster_input: self.paymaster_input,
            },
        };
        let execute = Execute {
            contract_address: self.to,
            value: self.value,
            calldata: self.data,
            factory_deps: self.factory_deps,
        };

        Ok(L2Tx {
            execute,
            common_data,
            received_timestamp_ms: 0,
            raw_bytes: self.raw_bytes.map(Bytes::from),
        })
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::fee::encoding_len;

    use super::*;

    #[test]
    fn test_consistency_with_encoding_length() {
        let transaction = TransactionData {
            tx_type: 113,
            from: Address::random(),
            to: Some(Address::random()),
            gas_limit: U256::from(1u32),
            pubdata_price_limit: U256::from(1u32),
            max_fee_per_gas: U256::from(1u32),
            max_priority_fee_per_gas: U256::from(1u32),
            paymaster: Address::random(),
            nonce: U256::zero(),
            value: U256::zero(),
            // The reserved fields that are unique for different types of transactions.
            // E.g. nonce is currently used in all transaction, but it should not be mandatory
            // in the long run.
            reserved: [U256::zero(); 4],
            data: vec![0u8; 65],
            signature: vec![0u8; 75],
            // The factory deps provided with the transaction.
            // Note that *only hashes* of these bytecodes are signed by the user
            // and they are used in the ABI encoding of the struct.
            // TODO: include this into the tx signature as part of SMA-1010
            factory_deps: vec![vec![0u8; 32], vec![1u8; 32]],
            paymaster_input: vec![0u8; 85],
            reserved_dynamic: vec![0u8; 32],
            raw_bytes: None,
        };

        let assumed_encoded_len = encoding_len(65, 75, 2, 85, 32);

        let true_encoding_len = transaction.into_tokens().len();

        assert_eq!(assumed_encoded_len, true_encoding_len);
    }
}
