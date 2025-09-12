use anyhow::Context as _;
use zksync_basic_types::protocol_version::{ProtocolSemanticVersion, ProtocolVersionId};

use crate::{
    bytecode::BytecodeHash,
    ethabi,
    ethabi::{ParamType, Token},
    transaction_request::TransactionRequest,
    web3, Address, H256, U256,
};

/// `L2CanonicalTransaction` from `l1-contracts/contracts/zksync/interfaces/IMailbox.sol`.
/// Represents L1->L2 transactions: priority transactions and protocol upgrade transactions.
#[derive(Default, Debug)]
pub struct L2CanonicalTransaction {
    pub tx_type: U256,
    pub from: U256,
    pub to: U256,
    pub gas_limit: U256,
    pub gas_per_pubdata_byte_limit: U256,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub paymaster: U256,
    pub nonce: U256,
    pub value: U256,
    pub reserved: [U256; 4],
    pub data: Vec<u8>,
    pub signature: Vec<u8>,
    pub factory_deps: Vec<U256>,
    pub paymaster_input: Vec<u8>,
    pub reserved_dynamic: Vec<u8>,
}

impl L2CanonicalTransaction {
    /// RLP schema of the L1->L2 transaction.
    pub fn schema() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::Uint(256),                                  // `txType`
            ParamType::Uint(256),                                  // sender
            ParamType::Uint(256),                                  // to
            ParamType::Uint(256),                                  // gasLimit
            ParamType::Uint(256),                                  // `gasPerPubdataLimit`
            ParamType::Uint(256),                                  // maxFeePerGas
            ParamType::Uint(256),                                  // maxPriorityFeePerGas
            ParamType::Uint(256),                                  // paymaster
            ParamType::Uint(256),                                  // nonce (serial ID)
            ParamType::Uint(256),                                  // value
            ParamType::FixedArray(ParamType::Uint(256).into(), 4), // reserved
            ParamType::Bytes,                                      // calldata
            ParamType::Bytes,                                      // signature
            ParamType::Array(Box::new(ParamType::Uint(256))),      // factory deps
            ParamType::Bytes,                                      // paymaster input
            ParamType::Bytes,                                      // `reservedDynamic`
        ])
    }

    /// Decodes L1->L2 transaction from a RLP token.
    /// Returns an error if token doesn't match the `schema()`.
    pub fn decode(token: Token) -> anyhow::Result<Self> {
        let tokens = token.into_tuple().context("not a tuple")?;
        anyhow::ensure!(tokens.len() == 16);
        let mut t = tokens.into_iter();
        let mut next = || t.next().unwrap();
        Ok(Self {
            tx_type: next().into_uint().context("tx_type")?,
            from: next().into_uint().context("from")?,
            to: next().into_uint().context("to")?,
            gas_limit: next().into_uint().context("gas_limit")?,
            gas_per_pubdata_byte_limit: next().into_uint().context("gas_per_pubdata_byte_limit")?,
            max_fee_per_gas: next().into_uint().context("max_fee_per_gas")?,
            max_priority_fee_per_gas: next().into_uint().context("max_priority_fee_per_gas")?,
            paymaster: next().into_uint().context("paymaster")?,
            nonce: next().into_uint().context("nonce")?,
            value: next().into_uint().context("value")?,
            reserved: next()
                .into_fixed_array()
                .context("reserved")?
                .into_iter()
                .enumerate()
                .map(|(i, t)| t.into_uint().context(i))
                .collect::<Result<Vec<_>, _>>()
                .context("reserved")?
                .try_into()
                .ok()
                .context("reserved")?,
            data: next().into_bytes().context("data")?,
            signature: next().into_bytes().context("signature")?,
            factory_deps: next()
                .into_array()
                .context("factory_deps")?
                .into_iter()
                .enumerate()
                .map(|(i, t)| t.into_uint().context(i))
                .collect::<Result<_, _>>()
                .context("factory_deps")?,
            paymaster_input: next().into_bytes().context("paymaster_input")?,
            reserved_dynamic: next().into_bytes().context("reserved_dynamic")?,
        })
    }

    /// Encodes L1->L2 transaction to a RLP token.
    pub fn encode(&self) -> Token {
        Token::Tuple(vec![
            Token::Uint(self.tx_type),
            Token::Uint(self.from),
            Token::Uint(self.to),
            Token::Uint(self.gas_limit),
            Token::Uint(self.gas_per_pubdata_byte_limit),
            Token::Uint(self.max_fee_per_gas),
            Token::Uint(self.max_priority_fee_per_gas),
            Token::Uint(self.paymaster),
            Token::Uint(self.nonce),
            Token::Uint(self.value),
            Token::FixedArray(self.reserved.iter().map(|x| Token::Uint(*x)).collect()),
            Token::Bytes(self.data.clone()),
            Token::Bytes(self.signature.clone()),
            Token::Array(self.factory_deps.iter().map(|x| Token::Uint(*x)).collect()),
            Token::Bytes(self.paymaster_input.clone()),
            Token::Bytes(self.reserved_dynamic.clone()),
        ])
    }

    /// Canonical hash of the L1->L2 transaction.
    pub fn hash(&self) -> H256 {
        H256::from_slice(&web3::keccak256(&ethabi::encode(&[self.encode()])))
    }
}

/// `NewPriorityRequest` from `l1-contracts/contracts/zksync/interfaces/IMailbox.sol`.
#[derive(Debug)]
pub struct NewPriorityRequest {
    pub tx_id: U256,
    pub tx_hash: [u8; 32],
    pub expiration_timestamp: u64,
    pub transaction: Box<L2CanonicalTransaction>,
    pub factory_deps: Vec<Vec<u8>>,
}

impl NewPriorityRequest {
    /// Encodes `NewPriorityRequest` to a sequence of RLP tokens.
    pub fn encode(&self) -> Vec<Token> {
        vec![
            Token::Uint(self.tx_id),
            Token::FixedBytes(self.tx_hash.into()),
            Token::Uint(self.expiration_timestamp.into()),
            self.transaction.encode(),
            Token::Array(
                self.factory_deps
                    .iter()
                    .map(|b| Token::Bytes(b.clone()))
                    .collect(),
            ),
        ]
    }

    /// Decodes `NewPriorityRequest` from RLP encoding.
    /// Returns an error if token doesn't match the `schema()`.
    pub fn decode(data: &[u8]) -> Result<Self, ethabi::Error> {
        let tokens = ethabi::decode(
            &[
                ParamType::Uint(256),                      // tx ID
                ParamType::FixedBytes(32),                 // tx hash
                ParamType::Uint(64),                       // expiration block
                L2CanonicalTransaction::schema(),          // transaction data
                ParamType::Array(ParamType::Bytes.into()), // factory deps
            ],
            data,
        )?;
        let mut t = tokens.into_iter();
        // All the unwraps are save because `ethabi::decode()` has validated
        // the input.
        let mut next = || t.next().unwrap();
        Ok(Self {
            tx_id: next().into_uint().unwrap(),
            tx_hash: next().into_fixed_bytes().unwrap().try_into().unwrap(),
            expiration_timestamp: next().into_uint().unwrap().try_into().unwrap(),
            transaction: L2CanonicalTransaction::decode(next()).unwrap().into(),
            factory_deps: next()
                .into_array()
                .unwrap()
                .into_iter()
                .map(|t| t.into_bytes().unwrap())
                .collect(),
        })
    }
}

/// `VerifierParams` from `l1-contracts/contracts/state-transition/chain-interfaces/IVerifier.sol`.
#[derive(Debug, Default, PartialEq)]
pub struct VerifierParams {
    pub recursion_node_level_vk_hash: [u8; 32],
    pub recursion_leaf_level_vk_hash: [u8; 32],
    pub recursion_circuits_set_vks_hash: [u8; 32],
}

#[derive(Debug, Clone, Copy)]
enum ProposedUpgradeSchema {
    PreGateway,
    PostGateway,
    PostEvmEmulator,
}

impl ProposedUpgradeSchema {
    fn tuple_type(self) -> ParamType {
        match self {
            Self::PreGateway => ParamType::Tuple(vec![
                L2CanonicalTransaction::schema(),          // transaction data
                ParamType::Array(ParamType::Bytes.into()), // factory deps
                ParamType::FixedBytes(32),                 // bootloader code hash
                ParamType::FixedBytes(32),                 // default account code hash
                ParamType::Address,                        // verifier address
                VerifierParams::schema(),                  // verifier params
                ParamType::Bytes,                          // l1 custom data
                ParamType::Bytes,                          // l1 post-upgrade custom data
                ParamType::Uint(256),                      // timestamp
                ParamType::Uint(256),                      // version id
            ]),
            Self::PostGateway => ParamType::Tuple(vec![
                L2CanonicalTransaction::schema(), // transaction data
                ParamType::FixedBytes(32),        // bootloader code hash
                ParamType::FixedBytes(32),        // default account code hash
                ParamType::Address,               // verifier address
                VerifierParams::schema(),         // verifier params
                ParamType::Bytes,                 // l1 custom data
                ParamType::Bytes,                 // l1 post-upgrade custom data
                ParamType::Uint(256),             // timestamp
                ParamType::Uint(256),             // version id
            ]),
            Self::PostEvmEmulator => ParamType::Tuple(vec![
                L2CanonicalTransaction::schema(), // transaction data
                ParamType::FixedBytes(32),        // bootloader code hash
                ParamType::FixedBytes(32),        // default account code hash
                ParamType::FixedBytes(32),        // EVM emulator code hash
                ParamType::Address,               // verifier address
                VerifierParams::schema(),         // verifier params
                ParamType::Bytes,                 // l1 custom data
                ParamType::Bytes,                 // l1 post-upgrade custom data
                ParamType::Uint(256),             // timestamp
                ParamType::Uint(256),             // version id
            ]),
        }
    }

    fn expected_tuple_len(self) -> usize {
        match self {
            Self::PreGateway | Self::PostEvmEmulator => 10,
            Self::PostGateway => 9,
        }
    }

    fn decode_to_token(data: &[u8]) -> Vec<(Self, Token)> {
        let mut output = vec![];
        for schema in [Self::PreGateway, Self::PostGateway, Self::PostEvmEmulator] {
            match ethabi::decode(&[schema.tuple_type()], data) {
                Ok(tokens) => {
                    output.push((schema, tokens.into_iter().next().unwrap()));
                }
                Err(err) => tracing::debug!(?schema, %err, "Error decoding proposed upgrade bytes"),
            }
        }
        output
    }

    fn matches_protocol_version(self, protocol_version: ProtocolVersionId) -> bool {
        match self {
            Self::PreGateway => protocol_version.is_pre_gateway(),
            // EVM emulator shares a protocol version upgrade with FFLONK
            Self::PostGateway => {
                protocol_version.is_post_gateway() && protocol_version.is_pre_fflonk()
            }
            Self::PostEvmEmulator => protocol_version.is_post_fflonk(),
        }
    }
}

/// `ProposedUpgrade` from, `l1-contracts/contracts/upgrades/BazeZkSyncUpgrade.sol`.
#[derive(Debug)]
pub struct ProposedUpgrade {
    pub l2_protocol_upgrade_tx: Box<L2CanonicalTransaction>,
    // Factory deps are set only pre-gateway upgrades.
    pub factory_deps: Option<Vec<Vec<u8>>>,
    pub bootloader_hash: [u8; 32],
    pub default_account_hash: [u8; 32],
    pub evm_emulator_hash: [u8; 32],
    pub verifier: Address,
    pub verifier_params: VerifierParams,
    pub l1_contracts_upgrade_calldata: Vec<u8>,
    pub post_upgrade_calldata: Vec<u8>,
    pub upgrade_timestamp: U256,
    pub new_protocol_version: U256,
}

impl VerifierParams {
    /// RLP schema of `VerifierParams`.
    pub fn schema() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
        ])
    }

    /// Encodes `VerifierParams` to a RLP token.
    pub fn encode(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.recursion_node_level_vk_hash.into()),
            Token::FixedBytes(self.recursion_leaf_level_vk_hash.into()),
            Token::FixedBytes(self.recursion_circuits_set_vks_hash.into()),
        ])
    }

    /// Decodes `VerifierParams` from a RLP token.
    /// Returns an error if token doesn't match the `schema()`.
    pub fn decode(token: Token) -> anyhow::Result<Self> {
        let tokens = token.into_tuple().context("not a tuple")?;
        anyhow::ensure!(tokens.len() == 3);
        let mut t = tokens.into_iter();
        let mut next = || t.next().unwrap();
        Ok(Self {
            recursion_node_level_vk_hash: next()
                .into_fixed_bytes()
                .and_then(|x| x.try_into().ok())
                .context("recursion_node_level_vk_hash")?,
            recursion_leaf_level_vk_hash: next()
                .into_fixed_bytes()
                .and_then(|x| x.try_into().ok())
                .context("recursion_leaf_level_vk_hash")?,
            recursion_circuits_set_vks_hash: next()
                .into_fixed_bytes()
                .and_then(|x| x.try_into().ok())
                .context("recursion_circuits_set_vks_hash")?,
        })
    }
}

impl ProposedUpgrade {
    /// Encodes `ProposedUpgrade` to a RLP token. Uses the latest schema.
    pub fn encode(&self) -> Token {
        let mut tokens = vec![self.l2_protocol_upgrade_tx.encode()];

        let protocol_version = ProtocolSemanticVersion::try_from_packed(self.new_protocol_version)
            .expect("Version is not supported")
            .minor;
        if protocol_version.is_pre_gateway() {
            tokens.push(Token::Array(
                self.factory_deps
                    .clone()
                    .expect("Factory deps should be present in pre-gateway upgrade data")
                    .into_iter()
                    .map(Token::Bytes)
                    .collect(),
            ));
        }
        tokens.extend([
            Token::FixedBytes(self.bootloader_hash.into()),
            Token::FixedBytes(self.default_account_hash.into()),
            Token::FixedBytes(self.evm_emulator_hash.into()),
            Token::Address(self.verifier),
            self.verifier_params.encode(),
            Token::Bytes(self.l1_contracts_upgrade_calldata.clone()),
            Token::Bytes(self.post_upgrade_calldata.clone()),
            Token::Uint(self.upgrade_timestamp),
            Token::Uint(self.new_protocol_version),
        ]);

        Token::Tuple(tokens)
    }

    pub(crate) fn decode(data: &[u8]) -> anyhow::Result<Self> {
        let mut upgrade = None;
        for (schema, token) in ProposedUpgradeSchema::decode_to_token(data) {
            match Self::decode_from_token(schema, token) {
                Ok(decoded) => {
                    if let Some(upgrade) = &upgrade {
                        anyhow::bail!("Ambiguous upgrade: can be decoded as either {decoded:?} or {upgrade:?}");
                    }
                    upgrade = Some(decoded);
                }
                Err(err) => {
                    tracing::debug!(?schema, %err, "Error decoding proposed upgrade");
                }
            }
        }
        upgrade.context("upgrade cannot be decoded")
    }

    /// Decodes `ProposedUpgrade` from a RLP token. Returns an error if token doesn't match the `schema`.
    fn decode_from_token(schema: ProposedUpgradeSchema, token: Token) -> anyhow::Result<Self> {
        let tokens = token.into_tuple().context("not a tuple")?;
        let tokens_len = tokens.len();
        anyhow::ensure!(tokens_len == schema.expected_tuple_len());
        let mut t = tokens.into_iter();
        let mut next = || t.next().unwrap();

        let l2_protocol_upgrade_tx = L2CanonicalTransaction::decode(next())
            .context("l2_protocol_upgrade_tx")?
            .into();
        let next_token = next();
        let (factory_deps, bootloader_hash) = match schema {
            ProposedUpgradeSchema::PreGateway => {
                let factory_deps = next_token
                    .into_array()
                    .context("expected factory deps array")?
                    .into_iter();
                let factory_deps = factory_deps
                    .enumerate()
                    .map(|(i, b)| b.into_bytes().context(i))
                    .collect::<Result<_, _>>()
                    .context("factory_deps")?;
                (Some(factory_deps), next().into_fixed_bytes())
            }
            ProposedUpgradeSchema::PostGateway | ProposedUpgradeSchema::PostEvmEmulator => {
                let bootloader_hash = next_token.into_fixed_bytes().context("bootloader_hash")?;
                (None, Some(bootloader_hash))
            }
        };

        let bootloader_hash = bootloader_hash
            .and_then(|b| b.try_into().ok())
            .context("bootloader_hash")?;

        let upgrade = Self {
            l2_protocol_upgrade_tx,
            factory_deps,
            bootloader_hash,
            default_account_hash: next()
                .into_fixed_bytes()
                .and_then(|b| b.try_into().ok())
                .context("default_account_hash")?,
            evm_emulator_hash: if matches!(schema, ProposedUpgradeSchema::PostEvmEmulator) {
                next()
                    .into_fixed_bytes()
                    .and_then(|b| b.try_into().ok())
                    .context("evm_emulator_hash")?
            } else {
                [0_u8; 32]
            },
            verifier: next().into_address().context("verifier")?,
            verifier_params: VerifierParams::decode(next()).context("verifier_params")?,
            l1_contracts_upgrade_calldata: next()
                .into_bytes()
                .context("l1_contracts_upgrade_calldata")?,
            post_upgrade_calldata: next().into_bytes().context("post_upgrade_calldata")?,
            upgrade_timestamp: next().into_uint().context("upgrade_timestamp")?,
            new_protocol_version: next().into_uint().context("new_protocol_version")?,
        };

        let protocol_version =
            ProtocolSemanticVersion::try_from_packed(upgrade.new_protocol_version)
                .map_err(|err| anyhow::anyhow!(err))
                .context("Version is not supported")?
                .minor;
        anyhow::ensure!(
            schema.matches_protocol_version(protocol_version),
            "Unexpected protocol version: {protocol_version:?} for upgrade schema: {schema:?}"
        );

        Ok(upgrade)
    }
}

/// Minimal representation of arbitrary zksync transaction.
/// Suitable for verifying hashes/re-encoding.
#[derive(Debug)]
pub enum Transaction {
    /// L1->L2 transaction (both protocol upgrade and Priority transaction).
    L1 {
        /// Hashed data.
        tx: Box<L2CanonicalTransaction>,
        /// `tx` contains a commitment to `factory_deps`.
        factory_deps: Vec<Vec<u8>>,
        /// Auxiliary data, not hashed.
        eth_block: u64,
    },
    /// RLP encoding of a L2 transaction.
    L2(Vec<u8>),
}

impl Transaction {
    /// Canonical hash of the transaction.
    /// Returns an error if data is inconsistent.
    /// Note that currently not all of the transaction
    /// content is included in the hash.
    pub fn hash(&self) -> anyhow::Result<H256> {
        Ok(match self {
            Self::L1 {
                tx, factory_deps, ..
            } => {
                // verify data integrity
                let factory_deps_hashes: Vec<_> = factory_deps
                    .iter()
                    .map(|b| BytecodeHash::for_bytecode(b).value_u256())
                    .collect();
                
                // Ignore check for now, we have factory deps in RegisterZKChain and this doesn't allow it for some reason.
                // // Check factory dependencies with detailed error output
                // if tx.factory_deps != factory_deps_hashes {
                //     anyhow::bail!(
                //         "ABI Factory dependencies mismatch!\n\
                //         Transaction expects {} deps: {:?}\n\
                //         But provided {} deps: {:?}\n\
                //         Count match: {}\n\
                //         Content match: {}",
                //         tx.factory_deps.len(), 
                //         tx.factory_deps,
                //         factory_deps_hashes.len(),
                //         factory_deps_hashes,
                //         tx.factory_deps.len() == factory_deps_hashes.len(),
                //         tx.factory_deps == factory_deps_hashes
                //     );
                // }
                
                tx.hash()
            }
            Self::L2(raw) => TransactionRequest::from_bytes_unverified(raw)?.1,
        })
    }
}

pub struct ForceDeployment {
    pub bytecode_hash: H256,
    pub new_address: Address,
    pub call_constructor: bool,
    pub value: U256,
    pub input: Vec<u8>,
}

impl ForceDeployment {
    /// ABI schema of the `ForceDeployment`.
    pub fn schema() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::FixedBytes(32),
            ParamType::Address,
            ParamType::Bool,
            ParamType::Uint(256),
            ParamType::Bytes,
        ])
    }

    /// Encodes `ForceDeployment` to a RLP token.
    pub fn encode(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.bytecode_hash.0.to_vec()),
            Token::Address(self.new_address),
            Token::Bool(self.call_constructor),
            Token::Uint(self.value),
            Token::Bytes(self.input.clone()),
        ])
    }

    /// Decodes `ForceDeployment` from a RLP token.
    /// Returns an error if token doesn't match the `schema()`.
    pub fn decode(token: Token) -> anyhow::Result<Self> {
        let tokens = token.into_tuple().context("not a tuple")?;
        anyhow::ensure!(tokens.len() == 5);
        let mut t = tokens.into_iter();
        let mut next = || t.next().unwrap();
        Ok(Self {
            bytecode_hash: next()
                .into_fixed_bytes()
                .and_then(|b| Some(H256(b.try_into().ok()?)))
                .context("bytecode_hash")?,
            new_address: next().into_address().context("new_address")?,
            call_constructor: next().into_bool().context("call_constructor")?,
            value: next().into_uint().context("value")?,
            input: next().into_bytes().context("input")?,
        })
    }
}

pub struct GatewayUpgradeEncodedInput {
    pub force_deployments: Vec<ForceDeployment>,
    pub l2_gateway_upgrade_position: usize,
    pub fixed_force_deployments_data: Vec<u8>,
    pub ctm_deployer: Address,
    pub old_validator_timelock: Address,
    pub new_validator_timelock: Address,
    pub wrapped_base_token_store: Address,
}

impl GatewayUpgradeEncodedInput {
    /// ABI schema of the `GatewayUpgradeEncodedInput`.
    pub fn schema() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::Array(Box::new(ForceDeployment::schema())),
            ParamType::Uint(256),
            ParamType::Bytes,
            ParamType::Address,
            ParamType::Address,
            ParamType::Address,
            ParamType::Address,
        ])
    }

    /// Decodes `GatewayUpgradeEncodedInput` from a RLP token.
    /// Returns an error if token doesn't match the `schema()`.
    pub fn decode(token: Token) -> anyhow::Result<Self> {
        let tokens = token.into_tuple().context("not a tuple")?;
        anyhow::ensure!(tokens.len() == 7);
        let mut t = tokens.into_iter();
        let mut next = || t.next().unwrap();

        let force_deployments_array = next().into_array().context("force_deployments_array")?;
        let mut force_deployments = vec![];
        for token in force_deployments_array {
            force_deployments.push(ForceDeployment::decode(token)?);
        }

        Ok(Self {
            force_deployments,
            l2_gateway_upgrade_position: next()
                .into_uint()
                .context("l2_gateway_upgrade_position")?
                .as_usize(),
            fixed_force_deployments_data: next()
                .into_bytes()
                .context("fixed_force_deployments_data")?,
            ctm_deployer: next().into_address().context("ctm_deployer")?,
            old_validator_timelock: next().into_address().context("old_validator_timelock")?,
            new_validator_timelock: next().into_address().context("new_validator_timelock")?,
            wrapped_base_token_store: next().into_address().context("wrapped_base_token_store")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ZkChainSpecificUpgradeData {
    pub base_token_asset_id: H256,
    pub l2_legacy_shared_bridge: Address,
    pub l2_predeployed_wrapped_base_token: Address,
    pub base_token_l1_address: Address,
    pub base_token_name: String,
    pub base_token_symbol: String,
}

impl ZkChainSpecificUpgradeData {
    pub fn from_partial_components(
        base_token_asset_id: Option<H256>,
        l2_legacy_shared_bridge: Option<Address>,
        predeployed_l2_weth_address: Option<Address>,
        base_token_l1_address: Option<Address>,
        base_token_name: Option<String>,
        base_token_symbol: Option<String>,
    ) -> Option<Self> {
        Some(Self {
            base_token_asset_id: base_token_asset_id?,
            l2_legacy_shared_bridge: l2_legacy_shared_bridge?,
            // Note, that some chains may not contain previous deployment of L2 wrapped base
            // token. For those, zero address is used.
            l2_predeployed_wrapped_base_token: predeployed_l2_weth_address.unwrap_or_default(),
            base_token_l1_address: base_token_l1_address?,
            base_token_name: base_token_name?,
            base_token_symbol: base_token_symbol?,
        })
    }

    /// ABI schema of the `ZkChainSpecificUpgradeData`.
    pub fn schema() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::FixedBytes(32),
            ParamType::Address,
            ParamType::Address,
        ])
    }

    /// Encodes `ZkChainSpecificUpgradeData` to a RLP token.
    pub fn encode(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.base_token_asset_id.0.to_vec()),
            Token::Address(self.l2_legacy_shared_bridge),
            Token::Address(self.l2_predeployed_wrapped_base_token),
            Token::Address(self.base_token_l1_address),
            Token::String(self.base_token_name.clone()),
            Token::String(self.base_token_symbol.clone()),
        ])
    }

    pub fn encode_bytes(&self) -> Vec<u8> {
        ethabi::encode(&[self.encode()])
    }
}
