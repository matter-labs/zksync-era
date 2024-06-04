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
use zksync_utils::{h256_to_u256, u256_to_account_address};

use crate::{
    ethabi::{decode, encode, ParamType, Token},
    helpers::unix_timestamp_ms,
    web3::{keccak256, Log},
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

fn get_transaction_param_type() -> ParamType {
    ParamType::Tuple(vec![
        ParamType::Uint(256),                                     // `txType`
        ParamType::Uint(256),                                     // sender
        ParamType::Uint(256),                                     // to
        ParamType::Uint(256),                                     // gasLimit
        ParamType::Uint(256),                                     // `gasPerPubdataLimit`
        ParamType::Uint(256),                                     // maxFeePerGas
        ParamType::Uint(256),                                     // maxPriorityFeePerGas
        ParamType::Uint(256),                                     // paymaster
        ParamType::Uint(256),                                     // nonce (serial ID)
        ParamType::Uint(256),                                     // value
        ParamType::FixedArray(Box::new(ParamType::Uint(256)), 4), // reserved
        ParamType::Bytes,                                         // calldata
        ParamType::Bytes,                                         // signature
        ParamType::Array(Box::new(ParamType::Uint(256))),         // factory deps
        ParamType::Bytes,                                         // paymaster input
        ParamType::Bytes,                                         // `reservedDynamic`
    ])
}

impl ProtocolUpgrade {
    fn try_from_decoded_tokens(tokens: Vec<ethabi::Token>) -> Result<Self, crate::ethabi::Error> {
        let init_calldata = tokens[2].clone().into_bytes().unwrap();

        let transaction_param_type: ParamType = get_transaction_param_type();
        let verifier_params_type = ParamType::Tuple(vec![
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
        ]);

        let mut decoded = decode(
            &[ParamType::Tuple(vec![
                transaction_param_type,                       // transaction data
                ParamType::Array(Box::new(ParamType::Bytes)), // factory deps
                ParamType::FixedBytes(32),                    // bootloader code hash
                ParamType::FixedBytes(32),                    // default account code hash
                ParamType::Address,                           // verifier address
                verifier_params_type,                         // verifier params
                ParamType::Bytes,                             // l1 custom data
                ParamType::Bytes,                             // l1 post-upgrade custom data
                ParamType::Uint(256),                         // timestamp
                ParamType::Uint(256),                         // version id
            ])],
            init_calldata
                .get(4..)
                .ok_or(crate::ethabi::Error::InvalidData)?,
        )?;

        let Token::Tuple(mut decoded) = decoded.remove(0) else {
            unreachable!();
        };

        let Token::Tuple(transaction) = decoded.remove(0) else {
            unreachable!()
        };

        let factory_deps = decoded.remove(0).into_array().unwrap();

        let tx = ProtocolUpgradeTx::decode_tx(transaction, factory_deps);
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
) -> Result<(ProtocolVersionId, ProtocolUpgradeTx), crate::ethabi::Error> {
    let transaction_param_type: ParamType = get_transaction_param_type();

    let Token::Tuple(transaction) = decode(&[transaction_param_type], &event.data.0)?.remove(0)
    else {
        unreachable!()
    };

    let full_version_id = h256_to_u256(event.topics[2]);
    let protocol_version = ProtocolVersionId::try_from_packed_semver(full_version_id)
        .unwrap_or_else(|_| panic!("Version is not supported, packed version: {full_version_id}"));

    let factory_deps: Vec<Token> = Vec::new();

    let upgrade_tx =
        ProtocolUpgradeTx::decode_tx(transaction, factory_deps).expect("Upgrade tx is missing");

    Ok((protocol_version, upgrade_tx))
}

impl ProtocolUpgradeTx {
    pub fn decode_tx(
        mut transaction: Vec<Token>,
        factory_deps: Vec<Token>,
    ) -> Option<ProtocolUpgradeTx> {
        let canonical_tx_hash = H256(keccak256(&encode(&[Token::Tuple(transaction.clone())])));
        assert_eq!(transaction.len(), 16);

        let tx_type = transaction.remove(0).into_uint().unwrap();
        if tx_type == U256::zero() {
            // There is no upgrade tx.
            return None;
        }

        assert_eq!(
            tx_type,
            PROTOCOL_UPGRADE_TX_TYPE.into(),
            "Unexpected tx type {} when decoding upgrade",
            tx_type
        );

        // There is an upgrade tx. Decoding it.
        let sender = transaction.remove(0).into_uint().unwrap();
        let sender = u256_to_account_address(&sender);

        let contract_address = transaction.remove(0).into_uint().unwrap();
        let contract_address = u256_to_account_address(&contract_address);

        let gas_limit = transaction.remove(0).into_uint().unwrap();

        let gas_per_pubdata_limit = transaction.remove(0).into_uint().unwrap();

        let max_fee_per_gas = transaction.remove(0).into_uint().unwrap();

        let max_priority_fee_per_gas = transaction.remove(0).into_uint().unwrap();
        assert_eq!(max_priority_fee_per_gas, U256::zero());

        let paymaster = transaction.remove(0).into_uint().unwrap();
        let paymaster = u256_to_account_address(&paymaster);
        assert_eq!(paymaster, Address::zero());

        let upgrade_id = transaction.remove(0).into_uint().unwrap();

        let msg_value = transaction.remove(0).into_uint().unwrap();

        let reserved = transaction
            .remove(0)
            .into_fixed_array()
            .unwrap()
            .into_iter()
            .map(|token| token.into_uint().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(reserved.len(), 4);

        let to_mint = reserved[0];
        let refund_recipient = u256_to_account_address(&reserved[1]);

        // All other reserved fields should be zero
        for item in reserved.iter().skip(2) {
            assert_eq!(item, &U256::zero());
        }

        let calldata = transaction.remove(0).into_bytes().unwrap();

        let signature = transaction.remove(0).into_bytes().unwrap();
        assert_eq!(signature.len(), 0);

        let _factory_deps_hashes = transaction.remove(0).into_array().unwrap();

        let paymaster_input = transaction.remove(0).into_bytes().unwrap();
        assert_eq!(paymaster_input.len(), 0);

        // TODO (SMA-1621): check that `reservedDynamic` are constructed correctly.
        let reserved_dynamic = transaction.remove(0).into_bytes().unwrap();
        assert_eq!(reserved_dynamic.len(), 0);

        let common_data = ProtocolUpgradeTxCommonData {
            canonical_tx_hash,
            sender,
            upgrade_id: (upgrade_id.as_u32() as u16).try_into().unwrap(),
            to_mint,
            refund_recipient,
            gas_limit,
            max_fee_per_gas,
            gas_per_pubdata_limit,
            eth_block: 0,
        };

        let factory_deps = factory_deps
            .into_iter()
            .map(|t| t.into_bytes().unwrap())
            .collect();

        let execute = Execute {
            contract_address,
            calldata: calldata.to_vec(),
            factory_deps: Some(factory_deps),
            value: msg_value,
        };

        Some(ProtocolUpgradeTx {
            common_data,
            execute,
            received_timestamp_ms: unix_timestamp_ms(),
        })
    }
}

impl TryFrom<Call> for ProtocolUpgrade {
    type Error = crate::ethabi::Error;

    fn try_from(call: Call) -> Result<Self, Self::Error> {
        let Call { data, .. } = call;

        if data.len() < 4 {
            return Err(crate::ethabi::Error::InvalidData);
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

        ProtocolUpgrade::try_from_decoded_tokens(diamond_cut_tokens)
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
        let mut decoded = decode(&[ParamType::Uint(256), operation_param_type], &event.data.0)?;
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

// TODO(PLA-962): remove once all nodes start treating the deprecated fields as optional.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProtocolUpgradeTxCommonDataSerde {
    pub sender: Address,
    pub upgrade_id: ProtocolVersionId,
    pub max_fee_per_gas: U256,
    pub gas_limit: U256,
    pub gas_per_pubdata_limit: U256,
    pub canonical_tx_hash: H256,
    pub to_mint: U256,
    pub refund_recipient: Address,

    /// DEPRECATED.
    #[serde(default)]
    pub eth_hash: H256,
    #[serde(default)]
    pub eth_block: u64,
}

#[derive(Default, Debug, Clone, PartialEq)]
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

impl serde::Serialize for ProtocolUpgradeTxCommonData {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ProtocolUpgradeTxCommonDataSerde {
            sender: self.sender,
            upgrade_id: self.upgrade_id,
            max_fee_per_gas: self.max_fee_per_gas,
            gas_limit: self.gas_limit,
            gas_per_pubdata_limit: self.gas_per_pubdata_limit,
            canonical_tx_hash: self.canonical_tx_hash,
            to_mint: self.to_mint,
            refund_recipient: self.refund_recipient,

            /// DEPRECATED.
            eth_hash: H256::default(),
            eth_block: self.eth_block,
        }
        .serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for ProtocolUpgradeTxCommonData {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let x = ProtocolUpgradeTxCommonDataSerde::deserialize(d)?;
        Ok(Self {
            sender: x.sender,
            upgrade_id: x.upgrade_id,
            max_fee_per_gas: x.max_fee_per_gas,
            gas_limit: x.gas_limit,
            gas_per_pubdata_limit: x.gas_per_pubdata_limit,
            canonical_tx_hash: x.canonical_tx_hash,
            to_mint: x.to_mint,
            refund_recipient: x.refund_recipient,

            // DEPRECATED.
            eth_block: x.eth_block,
        })
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
        let event_data = encode(&[Token::Uint(U256::zero()), operation_token]);

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
