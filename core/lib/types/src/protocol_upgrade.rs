use std::convert::{TryFrom, TryInto};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zksync_basic_types::protocol_version::{
    L1VerifierConfig, ProtocolSemanticVersion, ProtocolVersionId, VerifierParams,
};
use zksync_contracts::{BaseSystemContractsHashes, DIAMOND_CUT};

use crate::{
    abi::{self, ForceDeployment, GatewayUpgradeEncodedInput, ZkChainSpecificUpgradeData},
    ethabi::{self, ParamType, Token},
    h256_to_u256, u256_to_h256,
    web3::Log,
    Address, Execute, ExecuteTransactionCommon, Transaction, TransactionType, H256, U256,
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
}

impl std::fmt::Debug for Call {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("target", &self.target)
            .field("value", &self.value)
            .field("data", &hex::encode(&self.data))
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
    /// New EVM emulator code hash
    pub evm_emulator_code_hash: Option<H256>,
    /// New verifier params.
    pub verifier_params: Option<VerifierParams>,
    /// New verifier address.
    pub verifier_address: Option<Address>,
    /// Timestamp after which upgrade can be executed.
    pub timestamp: u64,
    /// L2 upgrade transaction.
    pub tx: Option<ProtocolUpgradeTx>,
}

impl From<VerifierParams> for abi::VerifierParams {
    fn from(x: VerifierParams) -> Self {
        Self {
            recursion_node_level_vk_hash: x.recursion_node_level_vk_hash.into(),
            recursion_leaf_level_vk_hash: x.recursion_node_level_vk_hash.into(),
            recursion_circuits_set_vks_hash: x.recursion_circuits_set_vks_hash.into(),
        }
    }
}

impl From<abi::VerifierParams> for VerifierParams {
    fn from(x: abi::VerifierParams) -> Self {
        Self {
            recursion_node_level_vk_hash: x.recursion_node_level_vk_hash.into(),
            recursion_leaf_level_vk_hash: x.recursion_node_level_vk_hash.into(),
            recursion_circuits_set_vks_hash: x.recursion_circuits_set_vks_hash.into(),
        }
    }
}

/// Protocol upgrade transactions do not contain preimages within them.
/// Instead, they are expected to be known and need to be fetched, typically from L1.
#[async_trait::async_trait]
pub trait ProtocolUpgradePreimageOracle: Send + Sync {
    async fn get_protocol_upgrade_preimages(
        &self,
        hashes: Vec<H256>,
    ) -> anyhow::Result<Vec<Vec<u8>>>;
}

/// Some upgrades have chain-dependent calldata that has to be prepared properly.
async fn prepare_upgrade_call(
    proposed_upgrade: &abi::ProposedUpgrade,
    chain_specific: Option<ZkChainSpecificUpgradeData>,
) -> anyhow::Result<Vec<u8>> {
    // No upgrade
    if proposed_upgrade.l2_protocol_upgrade_tx.tx_type == U256::zero() {
        return Ok(vec![]);
    }

    let minor_version = proposed_upgrade.l2_protocol_upgrade_tx.nonce;
    if ProtocolVersionId::try_from(minor_version.as_u32() as u16).unwrap()
        != ProtocolVersionId::gateway_upgrade()
    {
        // We'll just keep it the same for non-Gateway upgrades
        return Ok(proposed_upgrade.l2_protocol_upgrade_tx.data.clone());
    }

    // For gateway upgrade, things are bit more complex.
    // The source of truth for the code below is the one that is present in
    // `GatewayUpgrade.sol`.
    let mut encoded_input = GatewayUpgradeEncodedInput::decode(
        ethabi::decode(
            &[GatewayUpgradeEncodedInput::schema()],
            &proposed_upgrade.post_upgrade_calldata,
        )?[0]
            .clone(),
    )?;

    let gateway_upgrade_calldata = ethabi::encode(&[
        Token::Address(encoded_input.ctm_deployer),
        Token::Bytes(encoded_input.fixed_force_deployments_data),
        Token::Bytes(chain_specific.context("chain_specific")?.encode_bytes()),
    ]);

    // May not be very idiomatic, but we do it in the same way as it was done in Solidity
    // for easier review
    encoded_input.force_deployments[encoded_input.l2_gateway_upgrade_position].input =
        gateway_upgrade_calldata;

    let force_deployments_as_tokens: Vec<_> = encoded_input
        .force_deployments
        .iter()
        .map(ForceDeployment::encode)
        .collect();

    let full_data = zksync_contracts::deployer_contract()
        .function("forceDeployOnAddresses")
        .unwrap()
        .encode_input(&[Token::Array(force_deployments_as_tokens)])
        .unwrap();

    Ok(full_data)
}

impl ProtocolUpgrade {
    pub async fn try_from_diamond_cut(
        diamond_cut_data: &[u8],
        preimage_oracle: impl ProtocolUpgradePreimageOracle,
        chain_specific: Option<ZkChainSpecificUpgradeData>,
    ) -> anyhow::Result<Self> {
        // Unwraps are safe because we have validated the input against the function signature.
        let diamond_cut_tokens = DIAMOND_CUT.decode_input(diamond_cut_data)?[0]
            .clone()
            .into_tuple()
            .unwrap();
        Self::try_from_init_calldata(
            &diamond_cut_tokens[2].clone().into_bytes().unwrap(),
            preimage_oracle,
            chain_specific,
        )
        .await
    }

    /// `l1-contracts/contracts/state-transition/libraries/diamond.sol:DiamondCutData.initCalldata`
    async fn try_from_init_calldata(
        init_calldata: &[u8],
        preimage_oracle: impl ProtocolUpgradePreimageOracle,
        chain_specific: Option<ZkChainSpecificUpgradeData>,
    ) -> anyhow::Result<Self> {
        let raw_data = init_calldata.get(4..).context("need >= 4 bytes")?;
        let mut upgrade =
            abi::ProposedUpgrade::decode(raw_data).context("ProposedUpgrade::decode()")?;

        let bootloader_hash = H256(upgrade.bootloader_hash);
        let default_account_hash = H256(upgrade.default_account_hash);
        let evm_emulator_hash = H256(upgrade.evm_emulator_hash);

        let version = ProtocolSemanticVersion::try_from_packed(upgrade.new_protocol_version)
            .map_err(|err| anyhow::format_err!("Version is not supported: {err}"))?;
        let tx = if upgrade.l2_protocol_upgrade_tx.tx_type != U256::zero() {
            let factory_deps = if version.minor.is_pre_gateway() {
                upgrade.factory_deps.clone().unwrap()
            } else {
                preimage_oracle
                    .get_protocol_upgrade_preimages(
                        upgrade
                            .l2_protocol_upgrade_tx
                            .factory_deps
                            .iter()
                            .map(|&x| u256_to_h256(x))
                            .collect(),
                    )
                    .await?
            };

            upgrade.l2_protocol_upgrade_tx.data =
                prepare_upgrade_call(&upgrade, chain_specific).await?;

            Some(
                Transaction::from_abi(
                    abi::Transaction::L1 {
                        tx: upgrade.l2_protocol_upgrade_tx,
                        factory_deps,
                        eth_block: 0,
                    },
                    false,
                )
                .context("Transaction::try_from()")?
                .try_into()
                .map_err(|err| anyhow::format_err!("try_into::<ProtocolUpgradeTx>(): {err}"))?,
            )
        } else {
            None
        };

        Ok(Self {
            version,
            bootloader_code_hash: (bootloader_hash != H256::zero()).then_some(bootloader_hash),
            default_account_code_hash: (default_account_hash != H256::zero())
                .then_some(default_account_hash),
            evm_emulator_code_hash: (evm_emulator_hash != H256::zero())
                .then_some(evm_emulator_hash),
            verifier_params: (upgrade.verifier_params != abi::VerifierParams::default())
                .then_some(upgrade.verifier_params.into()),
            verifier_address: (upgrade.verifier != Address::zero()).then_some(upgrade.verifier),
            timestamp: upgrade.upgrade_timestamp.try_into().unwrap(),
            tx,
        })
    }
}

pub fn decode_genesis_upgrade_event(
    event: Log,
) -> Result<(ProtocolVersionId, ProtocolUpgradeTx), ethabi::Error> {
    let tokens = ethabi::decode(
        &[
            abi::L2CanonicalTransaction::schema(),
            ParamType::Array(Box::new(ParamType::Bytes)),
        ],
        &event.data.0,
    )?;
    let mut t: std::vec::IntoIter<Token> = tokens.into_iter();
    let mut next = || t.next().unwrap();

    let tx = abi::L2CanonicalTransaction::decode(next()).unwrap();
    let factory_deps = next()
        .into_array()
        .context("factory_deps")
        .map_err(|_| ethabi::Error::InvalidData)?
        .into_iter()
        .enumerate()
        .map(|(i, t)| t.into_bytes().context(i))
        .collect::<Result<Vec<Vec<u8>>, _>>()
        .context("factory_deps")
        .map_err(|_| ethabi::Error::InvalidData)?;
    let full_version_id = h256_to_u256(event.topics[2]);
    let protocol_version = ProtocolVersionId::try_from_packed_semver(full_version_id)
        .unwrap_or_else(|_| panic!("Version is not supported, packed version: {full_version_id}"));
    Ok((
        protocol_version,
        Transaction::from_abi(
            abi::Transaction::L1 {
                tx: tx.into(),
                eth_block: event
                    .block_number
                    .expect("Event block number is missing")
                    .as_u64(),
                factory_deps,
            },
            true,
        )
        .unwrap()
        .try_into()
        .unwrap(),
    ))
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
        new_snark_wrapper_vk_hash: Option<H256>,
        new_fflonk_snark_wrapper_vk_hash: Option<H256>,
    ) -> ProtocolVersion {
        ProtocolVersion {
            version: upgrade.version,
            timestamp: upgrade.timestamp,
            l1_verifier_config: L1VerifierConfig {
                snark_wrapper_vk_hash: new_snark_wrapper_vk_hash
                    .unwrap_or(self.l1_verifier_config.snark_wrapper_vk_hash),
                fflonk_snark_wrapper_vk_hash: new_fflonk_snark_wrapper_vk_hash
                    .or(self.l1_verifier_config.fflonk_snark_wrapper_vk_hash),
            },
            base_system_contracts_hashes: BaseSystemContractsHashes {
                bootloader: upgrade
                    .bootloader_code_hash
                    .unwrap_or(self.base_system_contracts_hashes.bootloader),
                default_aa: upgrade
                    .default_account_code_hash
                    .unwrap_or(self.base_system_contracts_hashes.default_aa),
                evm_emulator: upgrade
                    .evm_emulator_code_hash
                    .or(self.base_system_contracts_hashes.evm_emulator),
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
    /// Tx hash of the transaction in the ZKsync network. Calculated as the encoded transaction data hash.
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

            // DEPRECATED.
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
    use ethabi::Token;

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
            block_timestamp: Default::default(),
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
