use crate::ethabi::{decode, encode, ParamType, Token};
use crate::helpers::unix_timestamp_ms;
use crate::web3::contract::{tokens::Detokenize, Error};
use crate::web3::signing::keccak256;
use crate::{
    Address, Execute, ExecuteTransactionCommon, Log, Transaction, TransactionType, H256,
    PROTOCOL_UPGRADE_TX_TYPE, U256,
};
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_utils::u256_to_account_address;

#[repr(u16)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TryFromPrimitive,
)]
pub enum ProtocolVersionId {
    Version0 = 0,
    Version1,
    Version2,
    Version3,
    Version4,
    Version5,
    Version6,
    Version7,
    Version8,
    Version9,
    Version10,
    Version11,
    Version12,
    Version13,
}

impl ProtocolVersionId {
    pub fn latest() -> Self {
        Self::Version12
    }

    pub fn next() -> Self {
        Self::Version13
    }
}

impl Default for ProtocolVersionId {
    fn default() -> Self {
        Self::latest()
    }
}

impl TryFrom<U256> for ProtocolVersionId {
    type Error = String;

    fn try_from(value: U256) -> Result<Self, Self::Error> {
        if value > U256::from(u16::MAX) {
            Err(format!("unknown protocol version ID: {}", value))
        } else {
            (value.as_u32() as u16)
                .try_into()
                .map_err(|_| format!("unknown protocol version ID: {}", value))
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerifierParams {
    pub recursion_node_level_vk_hash: H256,
    pub recursion_leaf_level_vk_hash: H256,
    pub recursion_circuits_set_vks_hash: H256,
}

impl Detokenize for VerifierParams {
    fn from_tokens(tokens: Vec<Token>) -> Result<Self, Error> {
        if tokens.len() != 1 {
            return Err(Error::Abi(crate::ethabi::Error::InvalidData));
        }

        let tokens = match tokens[0].clone() {
            Token::Tuple(tokens) => tokens,
            _ => return Err(Error::Abi(crate::ethabi::Error::InvalidData)),
        };

        let vks_vec: Vec<H256> = tokens
            .into_iter()
            .map(|token| H256::from_slice(&token.into_fixed_bytes().unwrap()))
            .collect();
        Ok(VerifierParams {
            recursion_node_level_vk_hash: vks_vec[0],
            recursion_leaf_level_vk_hash: vks_vec[1],
            recursion_circuits_set_vks_hash: vks_vec[2],
        })
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct L1VerifierConfig {
    pub params: VerifierParams,
    pub recursion_scheduler_level_vk_hash: H256,
}

/// Protocol upgrade proposal from L1.
/// Most of the fields are optional meaning if value is none
/// then this field is not changed within an upgrade.
#[derive(Debug, Clone, Default)]
pub struct ProtocolUpgrade {
    /// New protocol version ID.
    pub id: ProtocolVersionId,
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

impl TryFrom<Log> for ProtocolUpgrade {
    type Error = crate::ethabi::Error;

    fn try_from(event: Log) -> Result<Self, Self::Error> {
        let facet_cut_param_type = ParamType::Tuple(vec![
            ParamType::Address,
            ParamType::Uint(8),
            ParamType::Bool,
            ParamType::Array(Box::new(ParamType::FixedBytes(4))),
        ]);
        let diamond_cut_data_param_type = ParamType::Tuple(vec![
            ParamType::Array(Box::new(facet_cut_param_type)),
            ParamType::Address,
            ParamType::Bytes,
        ]);
        let mut decoded = decode(
            &[diamond_cut_data_param_type, ParamType::FixedBytes(32)],
            &event.data.0,
        )?;

        let init_calldata = match decoded.remove(0) {
            Token::Tuple(tokens) => tokens[2].clone().into_bytes().unwrap(),
            _ => unreachable!(),
        };

        let transaction_param_type = ParamType::Tuple(vec![
            ParamType::Uint(256),                                     // txType
            ParamType::Uint(256),                                     // sender
            ParamType::Uint(256),                                     // to
            ParamType::Uint(256),                                     // gasLimit
            ParamType::Uint(256),                                     // gasPerPubdataLimit
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
            ParamType::Bytes,                                         // reservedDynamic
        ]);
        let verifier_params_type = ParamType::Tuple(vec![
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
        ]);

        let mut decoded = decode(
            &[ParamType::Tuple(vec![
                transaction_param_type,                       // transaction data
                ParamType::Array(Box::new(ParamType::Bytes)), //factory deps
                ParamType::FixedBytes(32),                    // bootloader code hash
                ParamType::FixedBytes(32),                    // default account code hash
                ParamType::Address,                           // verifier address
                verifier_params_type,                         // verifier params
                ParamType::Bytes,                             // l1 custom data
                ParamType::Bytes,                             // l1 post-upgrade custom data
                ParamType::Uint(256),                         // timestamp
                ParamType::Uint(256),                         // version id
                ParamType::Address,                           // allow list address
            ])],
            &init_calldata[4..],
        )?;

        let mut decoded = match decoded.remove(0) {
            Token::Tuple(x) => x,
            _ => unreachable!(),
        };

        let mut transaction = match decoded.remove(0) {
            Token::Tuple(x) => x,
            _ => unreachable!(),
        };
        let factory_deps = decoded.remove(0).into_array().unwrap();

        let tx = {
            let canonical_tx_hash = H256(keccak256(&encode(&[Token::Tuple(transaction.clone())])));

            assert_eq!(transaction.len(), 16);

            let tx_type = transaction.remove(0).into_uint().unwrap();
            if tx_type == PROTOCOL_UPGRADE_TX_TYPE.into() {
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

                let reserved_dynamic = transaction.remove(0).into_bytes().unwrap();
                assert_eq!(reserved_dynamic.len(), 0);

                let eth_hash = event
                    .transaction_hash
                    .expect("Event transaction hash is missing");
                let eth_block = event
                    .block_number
                    .expect("Event block number is missing")
                    .as_u64();

                let common_data = ProtocolUpgradeTxCommonData {
                    canonical_tx_hash,
                    sender,
                    upgrade_id: (upgrade_id.as_u32() as u16).try_into().unwrap(),
                    to_mint,
                    refund_recipient,
                    gas_limit,
                    max_fee_per_gas,
                    gas_per_pubdata_limit,
                    eth_hash,
                    eth_block,
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
            } else if tx_type == U256::zero() {
                // There is no upgrade tx.
                None
            } else {
                panic!("Unexpected tx type {} when decoding upgrade", tx_type);
            }
        };
        let bootloader_code_hash = H256::from_slice(&decoded.remove(0).into_fixed_bytes().unwrap());
        let default_account_code_hash =
            H256::from_slice(&decoded.remove(0).into_fixed_bytes().unwrap());
        let verifier_address = decoded.remove(0).into_address().unwrap();
        let mut verifier_params = match decoded.remove(0) {
            Token::Tuple(tx) => tx,
            _ => unreachable!(),
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
        let version_id = decoded.remove(0).into_uint().unwrap();
        if version_id > u16::MAX.into() {
            panic!("Version ID is too big, max expected is {}", u16::MAX);
        }

        let _allow_list_address = decoded.remove(0).into_address().unwrap();

        Ok(Self {
            id: ProtocolVersionId::try_from(version_id.as_u32() as u16)
                .expect("Version is not supported"),
            bootloader_code_hash: (bootloader_code_hash != H256::zero())
                .then_some(bootloader_code_hash),
            default_account_code_hash: (default_account_code_hash != H256::zero())
                .then_some(default_account_code_hash),
            verifier_params: (recursion_node_level_vk_hash != H256::zero()
                && recursion_leaf_level_vk_hash != H256::zero()
                && recursion_circuits_set_vks_hash != H256::zero())
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

#[derive(Debug, Clone, Default)]
pub struct ProtocolVersion {
    /// Protocol version ID
    pub id: ProtocolVersionId,
    /// Timestamp at which upgrade should be performed
    pub timestamp: u64,
    /// Verifier configuration
    pub l1_verifier_config: L1VerifierConfig,
    /// Hashes of base system contracts (bootloader and default account)
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    /// Verifier contract address on L1
    pub verifier_address: Address,
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
            id: upgrade.id,
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
            verifier_address: upgrade.verifier_address.unwrap_or(self.verifier_address),
            tx: upgrade.tx,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
