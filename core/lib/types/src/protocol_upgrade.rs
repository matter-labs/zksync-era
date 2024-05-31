use std::convert::{TryFrom, TryInto};

use serde::{Deserialize, Serialize};
use zksync_basic_types::{
    ethabi,
    protocol_version::{
        L1VerifierConfig, ProtocolSemanticVersion, ProtocolVersionId, VerifierParams,
    },
};
use zksync_contracts::{
    BaseSystemContractsHashes, ADMIN_EXECUTE_UPGRADE_FUNCTION,
    ADMIN_UPGRADE_CHAIN_FROM_VERSION_FUNCTION,
};
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256, u256_to_account_address};

use crate::{
    ethabi::{ParamType, Token},
    helpers::unix_timestamp_ms,
    l1::L2CanonicalTransaction,
    web3::Log,
    Address, Execute, ExecuteTransactionCommon, Transaction, TransactionType, H256,
    PROTOCOL_UPGRADE_TX_TYPE, U256,
};

/// Represents a call to be made during governance operation.
#[derive(Clone, Serialize, Deserialize)]
pub struct Call {
    /// The address to which the call will be made.
    pub target: Address,
    ///  The amount of Ether (in wei) to be sent along with the call.
    pub value: U256,
    /// The calldata to be executed on the `target` address.
    pub data: Vec<u8>,
    /// Hash of the corresponding Ethereum transaction. Size should be 32 bytes.
    pub eth_hash: H256,
    /// Block in which Ethereum transaction was included.
    pub eth_block: u64,
}

impl std::fmt::Debug for Call {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("target", &self.target)
            .field("value", &self.value)
            .field("data", &hex::encode(&self.data))
            .field("eth_hash", &self.eth_hash)
            .field("eth_block", &self.eth_block)
            .finish()
    }
}

/// Defines the structure of an operation that Governance contract executes.
#[derive(Debug, Clone, Default)]
pub struct GovernanceOperation {
    /// An array of `Call` structs, each representing a call to be made during the operation.
    pub calls: Vec<Call>,
    /// The hash of the predecessor operation, that should be executed before this operation.
    pub predecessor: H256,
    /// The value used for creating unique operation hashes.
    pub salt: H256,
}

/// Protocol upgrade proposal from L1.
/// Most of the fields are optional meaning if value is none
/// then this field is not changed within an upgrade.
#[derive(Debug, Clone, Default)]
pub struct ProtocolUpgrade {
    /// New protocol version ID.
    pub version: ProtocolSemanticVersion,
    /// New bootloader code hash.
    pub bootloader_code_hash: Option<H256>,
    /// New default account code hash.
    pub default_account_code_hash: Option<H256>,
    /// New verifier params.
    pub verifier_params: Option<VerifierParams>,
    /// New verifier address.
    pub verifier_address: Option<Address>,
    /// Timestamp after which upgrade can be executed.
    pub timestamp: u64,
    /// L2 upgrade transaction.
    pub tx: Option<ProtocolUpgradeTx>,
}

impl ProtocolUpgrade {
    fn try_from_decoded_tokens(
        tokens: Vec<ethabi::Token>,
        transaction_hash: H256,
        transaction_block_number: u64,
    ) -> Result<Self, ethabi::Error> {
        let init_calldata = tokens[2].clone().into_bytes().unwrap();

        let verifier_params_type = ParamType::Tuple(vec![
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
        ]);

        let mut decoded = ethabi::decode(
            &[ParamType::Tuple(vec![
                ParamType::Tuple(L2CanonicalTransaction::schema()), // transaction data
                ParamType::Array(ParamType::Bytes.into()),          // factory deps
                ParamType::FixedBytes(32),                          // bootloader code hash
                ParamType::FixedBytes(32),                          // default account code hash
                ParamType::Address,                                 // verifier address
                verifier_params_type,                               // verifier params
                ParamType::Bytes,                                   // l1 custom data
                ParamType::Bytes,                                   // l1 post-upgrade custom data
                ParamType::Uint(256),                               // timestamp
                ParamType::Uint(256),                               // version id
            ])],
            init_calldata
                .get(4..)
                .ok_or(crate::ethabi::Error::InvalidData)?,
        )?;

        let mut decoded = decoded.remove(0).into_tuple().unwrap();
        let transaction = decoded.remove(0).into_tuple().unwrap();
        let factory_deps = decoded.remove(0).into_array().unwrap();

        let tx = ProtocolUpgradeTx::decode_tx(
            transaction,
            transaction_hash,
            transaction_block_number,
            factory_deps,
        );
        let bootloader_code_hash = H256::from_slice(&decoded.remove(0).into_fixed_bytes().unwrap());
        let default_account_code_hash =
            H256::from_slice(&decoded.remove(0).into_fixed_bytes().unwrap());
        let verifier_address = decoded.remove(0).into_address().unwrap();
        let Token::Tuple(mut verifier_params) = decoded.remove(0) else {
            unreachable!()
        };
        let recursion_node_level_vk_hash =
            H256::from_slice(&verifier_params.remove(0).into_fixed_bytes().unwrap());
        let recursion_leaf_level_vk_hash =
            H256::from_slice(&verifier_params.remove(0).into_fixed_bytes().unwrap());
        let recursion_circuits_set_vks_hash =
            H256::from_slice(&verifier_params.remove(0).into_fixed_bytes().unwrap());

        let _l1_custom_data = decoded.remove(0);
        let _l1_post_upgrade_custom_data = decoded.remove(0);
        let timestamp = decoded.remove(0).into_uint().unwrap();
        let packed_protocol_semver = decoded.remove(0).into_uint().unwrap();

        Ok(Self {
            version: ProtocolSemanticVersion::try_from_packed(packed_protocol_semver)
                .expect("Version is not supported"),
            bootloader_code_hash: (bootloader_code_hash != H256::zero())
                .then_some(bootloader_code_hash),
            default_account_code_hash: (default_account_code_hash != H256::zero())
                .then_some(default_account_code_hash),
            verifier_params: (recursion_node_level_vk_hash != H256::zero()
                || recursion_leaf_level_vk_hash != H256::zero()
                || recursion_circuits_set_vks_hash != H256::zero())
            .then_some(VerifierParams {
                recursion_node_level_vk_hash,
                recursion_leaf_level_vk_hash,
                recursion_circuits_set_vks_hash,
            }),
            verifier_address: (verifier_address != Address::zero()).then_some(verifier_address),
            timestamp: timestamp.as_u64(),
            tx,
        })
    }
}

pub fn decode_set_chain_id_event(
    event: Log,
) -> Result<(ProtocolVersionId, ProtocolUpgradeTx), ethabi::Error> {
    let transaction = ethabi::decode(
        &[ParamType::Tuple(L2CanonicalTransaction::schema())],
        &event.data.0,
    )?
    .remove(0)
    .into_tuple()
    .unwrap();

    let full_version_id = h256_to_u256(event.topics[2]);
    let protocol_version = ProtocolVersionId::try_from_packed_semver(full_version_id)
        .unwrap_or_else(|_| panic!("Version is not supported, packed version: {full_version_id}"));

    let eth_hash = event
        .transaction_hash
        .expect("Event transaction hash is missing");
    let eth_block = event
        .block_number
        .expect("Event block number is missing")
        .as_u64();

    let factory_deps: Vec<Token> = Vec::new();

    let upgrade_tx = ProtocolUpgradeTx::decode_tx(transaction, eth_hash, eth_block, factory_deps)
        .expect("Upgrade tx is missing");

    Ok((protocol_version, upgrade_tx))
}

impl ProtocolUpgradeTx {
    pub fn decode_tx(
        transaction: Vec<Token>,
        eth_hash: H256,
        eth_block: u64,
        factory_deps: Vec<Token>,
    ) -> Option<ProtocolUpgradeTx> {
        let transaction = L2CanonicalTransaction::decode(transaction).unwrap();
        if transaction.tx_type == U256::zero() {
            // There is no upgrade tx.
            return None;
        }
        assert_eq!(
            transaction.tx_type,
            PROTOCOL_UPGRADE_TX_TYPE.into(),
            "Unexpected tx type {} when decoding upgrade",
            transaction.tx_type
        );
        assert_eq!(transaction.max_priority_fee_per_gas, U256::zero());
        assert_eq!(transaction.paymaster, U256::zero());

        // All other reserved fields should be zero
        for item in &transaction.reserved[2..] {
            assert_eq!(item, &U256::zero());
        }
        assert!(transaction.signature.is_empty());
        assert!(transaction.paymaster_input.is_empty());
        assert!(transaction.reserved_dynamic.is_empty());
        // TODO (SMA-1621): check that `reservedDynamic` are constructed correctly.

        let common_data = ProtocolUpgradeTxCommonData {
            canonical_tx_hash: transaction.hash(),
            sender: u256_to_account_address(&transaction.from),
            upgrade_id: transaction.nonce.try_into().unwrap(),
            to_mint: transaction.reserved[0],
            refund_recipient: u256_to_account_address(&transaction.reserved[1]),
            gas_limit: transaction.gas_limit,
            max_fee_per_gas: transaction.max_fee_per_gas,
            gas_per_pubdata_limit: transaction.gas_per_pubdata_byte_limit,
            eth_hash,
            eth_block,
        };
        let factory_deps: Vec<_> = factory_deps
            .into_iter()
            .map(|t| t.into_bytes().unwrap())
            .collect();
        let factory_deps_hashes: Vec<_> = factory_deps
            .iter()
            .map(|b| h256_to_u256(hash_bytecode(b)))
            .collect();
        assert_eq!(transaction.factory_deps, factory_deps_hashes);
        let execute = Execute {
            contract_address: u256_to_account_address(&transaction.to),
            calldata: transaction.data,
            factory_deps: Some(factory_deps),
            value: transaction.value,
        };

        Some(ProtocolUpgradeTx {
            common_data,
            execute,
            received_timestamp_ms: unix_timestamp_ms(),
        })
    }
}

impl TryFrom<Call> for ProtocolUpgrade {
    type Error = ethabi::Error;

    fn try_from(call: Call) -> Result<Self, Self::Error> {
        let Call {
            data,
            eth_hash,
            eth_block,
            ..
        } = call;

        if data.len() < 4 {
            return Err(ethabi::Error::InvalidData);
        }

        let (signature, data) = data.split_at(4);

        let diamond_cut_tokens =
            if signature.to_vec() == ADMIN_EXECUTE_UPGRADE_FUNCTION.short_signature().to_vec() {
                ADMIN_EXECUTE_UPGRADE_FUNCTION
                    .decode_input(data)?
                    .pop()
                    .unwrap()
                    .into_tuple()
                    .unwrap()
            } else if signature.to_vec()
                == ADMIN_UPGRADE_CHAIN_FROM_VERSION_FUNCTION
                    .short_signature()
                    .to_vec()
            {
                let mut data = ADMIN_UPGRADE_CHAIN_FROM_VERSION_FUNCTION.decode_input(data)?;

                assert_eq!(
                    data.len(),
                    2,
                    "The second method is expected to accept exactly 2 arguments"
                );

                // The second item must be a tuple of diamond cut data
                data.pop().unwrap().into_tuple().unwrap()
            } else {
                return Err(crate::ethabi::Error::InvalidData);
            };

        ProtocolUpgrade::try_from_decoded_tokens(diamond_cut_tokens, eth_hash, eth_block)
    }
}

impl TryFrom<Log> for GovernanceOperation {
    type Error = crate::ethabi::Error;

    fn try_from(event: Log) -> Result<Self, Self::Error> {
        let call_param_type = ParamType::Tuple(vec![
            ParamType::Address,
            ParamType::Uint(256),
            ParamType::Bytes,
        ]);

        let operation_param_type = ParamType::Tuple(vec![
            ParamType::Array(Box::new(call_param_type)),
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
        ]);
        // Decode data.
        let mut decoded =
            ethabi::decode(&[ParamType::Uint(256), operation_param_type], &event.data.0)?;
        // Extract `GovernanceOperation` data.
        let mut decoded_governance_operation = decoded.remove(1).into_tuple().unwrap();

        let eth_hash = event
            .transaction_hash
            .expect("Event transaction hash is missing");
        let eth_block = event
            .block_number
            .expect("Event block number is missing")
            .as_u64();

        let calls = decoded_governance_operation.remove(0).into_array().unwrap();
        let predecessor = H256::from_slice(
            &decoded_governance_operation
                .remove(0)
                .into_fixed_bytes()
                .unwrap(),
        );
        let salt = H256::from_slice(
            &decoded_governance_operation
                .remove(0)
                .into_fixed_bytes()
                .unwrap(),
        );

        let calls = calls
            .into_iter()
            .map(|call| {
                let mut decoded_governance_operation = call.into_tuple().unwrap();

                Call {
                    target: decoded_governance_operation
                        .remove(0)
                        .into_address()
                        .unwrap(),
                    value: decoded_governance_operation.remove(0).into_uint().unwrap(),
                    data: decoded_governance_operation.remove(0).into_bytes().unwrap(),
                    eth_hash,
                    eth_block,
                }
            })
            .collect();

        Ok(Self {
            calls,
            predecessor,
            salt,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProtocolVersion {
    /// Protocol version ID
    pub version: ProtocolSemanticVersion,
    /// Timestamp at which upgrade should be performed
    pub timestamp: u64,
    /// Verifier configuration
    pub l1_verifier_config: L1VerifierConfig,
    /// Hashes of base system contracts (bootloader and default account)
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    /// L2 Upgrade transaction.
    pub tx: Option<ProtocolUpgradeTx>,
}

impl ProtocolVersion {
    /// Returns new protocol version parameters after applying provided upgrade.
    pub fn apply_upgrade(
        &self,
        upgrade: ProtocolUpgrade,
        new_scheduler_vk_hash: Option<H256>,
    ) -> ProtocolVersion {
        ProtocolVersion {
            version: upgrade.version,
            timestamp: upgrade.timestamp,
            l1_verifier_config: L1VerifierConfig {
                params: upgrade
                    .verifier_params
                    .unwrap_or(self.l1_verifier_config.params),
                recursion_scheduler_level_vk_hash: new_scheduler_vk_hash
                    .unwrap_or(self.l1_verifier_config.recursion_scheduler_level_vk_hash),
            },
            base_system_contracts_hashes: BaseSystemContractsHashes {
                bootloader: upgrade
                    .bootloader_code_hash
                    .unwrap_or(self.base_system_contracts_hashes.bootloader),
                default_aa: upgrade
                    .default_account_code_hash
                    .unwrap_or(self.base_system_contracts_hashes.default_aa),
            },
            tx: upgrade.tx,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolUpgradeTxCommonData {
    /// Sender of the transaction.
    pub sender: Address,
    /// ID of the upgrade.
    pub upgrade_id: ProtocolVersionId,
    /// The maximal fee per gas to be used for L1->L2 transaction
    pub max_fee_per_gas: U256,
    /// The maximum number of gas that a transaction can spend at a price of gas equals 1.
    pub gas_limit: U256,
    /// The maximum number of gas per 1 byte of pubdata.
    pub gas_per_pubdata_limit: U256,
    /// Hash of the corresponding Ethereum transaction. Size should be 32 bytes.
    pub eth_hash: H256,
    /// Block in which Ethereum transaction was included.
    pub eth_block: u64,
    /// Tx hash of the transaction in the zkSync network. Calculated as the encoded transaction data hash.
    pub canonical_tx_hash: H256,
    /// The amount of ETH that should be minted with this transaction
    pub to_mint: U256,
    /// The recipient of the refund of the transaction
    pub refund_recipient: Address,
}

impl ProtocolUpgradeTxCommonData {
    pub fn hash(&self) -> H256 {
        self.canonical_tx_hash
    }

    pub fn tx_format(&self) -> TransactionType {
        TransactionType::ProtocolUpgradeTransaction
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolUpgradeTx {
    pub execute: Execute,
    pub common_data: ProtocolUpgradeTxCommonData,
    pub received_timestamp_ms: u64,
}

impl From<ProtocolUpgradeTx> for Transaction {
    fn from(tx: ProtocolUpgradeTx) -> Self {
        let ProtocolUpgradeTx {
            execute,
            common_data,
            received_timestamp_ms,
        } = tx;
        Self {
            common_data: ExecuteTransactionCommon::ProtocolUpgrade(common_data),
            execute,
            received_timestamp_ms,
            raw_bytes: None,
        }
    }
}

impl TryFrom<Transaction> for ProtocolUpgradeTx {
    type Error = &'static str;

    fn try_from(value: Transaction) -> Result<Self, Self::Error> {
        let Transaction {
            common_data,
            execute,
            received_timestamp_ms,
            ..
        } = value;
        match common_data {
            ExecuteTransactionCommon::L1(_) => Err("Cannot convert L1Tx to ProtocolUpgradeTx"),
            ExecuteTransactionCommon::L2(_) => Err("Cannot convert L2Tx to ProtocolUpgradeTx"),
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => Ok(ProtocolUpgradeTx {
                execute,
                common_data,
                received_timestamp_ms,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governance_operation_from_log() {
        let call_token = Token::Tuple(vec![
            Token::Address(Address::random()),
            Token::Uint(U256::zero()),
            Token::Bytes(vec![1, 2, 3]),
        ]);
        let operation_token = Token::Tuple(vec![
            Token::Array(vec![call_token]),
            Token::FixedBytes(H256::random().0.to_vec()),
            Token::FixedBytes(H256::random().0.to_vec()),
        ]);
        let event_data = ethabi::encode(&[Token::Uint(U256::zero()), operation_token]);

        let correct_log = Log {
            address: Default::default(),
            topics: Default::default(),
            data: event_data.into(),
            block_hash: Default::default(),
            block_number: Some(1u64.into()),
            transaction_hash: Some(H256::random()),
            transaction_index: Default::default(),
            log_index: Default::default(),
            transaction_log_index: Default::default(),
            log_type: Default::default(),
            removed: Default::default(),
        };
        let decoded_op: GovernanceOperation = correct_log.clone().try_into().unwrap();
        assert_eq!(decoded_op.calls.len(), 1);

        let mut incorrect_log = correct_log;
        incorrect_log
            .data
            .0
            .truncate(incorrect_log.data.0.len() - 32);
        assert!(TryInto::<GovernanceOperation>::try_into(incorrect_log).is_err());
    }
}
