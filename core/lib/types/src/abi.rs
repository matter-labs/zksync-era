use anyhow::Context as _;
use zksync_utils::{bytecode::hash_bytecode, h256_to_u256};

use crate::{
    ethabi,
    ethabi::{ParamType, Token},
    transaction_request::TransactionRequest,
    web3, Address, H256, U256,
};

/// `Transaction` from `system-contracts/contracts/libraries/TransactionHelper.sol`.
/// `L2CanonicalTransaction` from `l1-contracts/contracts/zksync/interfaces/IMailbox.sol`.
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

    pub fn hash(&self) -> H256 {
        H256::from_slice(&web3::keccak256(&ethabi::encode(&[self.encode()])))
    }
}

#[derive(Debug)]
pub struct NewPriorityRequest {
    pub tx_id: U256,
    pub tx_hash: [u8; 32],
    pub expiration_timestamp: u64,
    pub transaction: Box<L2CanonicalTransaction>,
    pub factory_deps: Vec<Vec<u8>>,
}

impl NewPriorityRequest {
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

/// `l1-contracts/contracts/state-transition/chain-interfaces/IVerifier.sol:VerifierParams`.
#[derive(Default, PartialEq)]
pub struct VerifierParams {
    pub recursion_node_level_vk_hash: [u8; 32],
    pub recursion_leaf_level_vk_hash: [u8; 32],
    pub recursion_circuits_set_vks_hash: [u8; 32],
}

/// `l1-contracts/contracts/upgrades/BazeZkSyncUpgrade.sol:ProposedUpgrade`.
pub struct ProposedUpgrade {
    pub l2_protocol_upgrade_tx: Box<L2CanonicalTransaction>,
    pub factory_deps: Vec<Vec<u8>>,
    pub bootloader_hash: [u8; 32],
    pub default_account_hash: [u8; 32],
    pub verifier: Address,
    pub verifier_params: VerifierParams,
    pub l1_contracts_upgrade_calldata: Vec<u8>,
    pub post_upgrade_calldata: Vec<u8>,
    pub upgrade_timestamp: U256,
    pub new_protocol_version: U256,
}

impl VerifierParams {
    pub fn schema() -> ParamType {
        ParamType::Tuple(vec![
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
            ParamType::FixedBytes(32),
        ])
    }

    pub fn encode(&self) -> Token {
        Token::Tuple(vec![
            Token::FixedBytes(self.recursion_node_level_vk_hash.into()),
            Token::FixedBytes(self.recursion_leaf_level_vk_hash.into()),
            Token::FixedBytes(self.recursion_circuits_set_vks_hash.into()),
        ])
    }

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
    pub fn schema() -> ParamType {
        ParamType::Tuple(vec![
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
        ])
    }

    pub fn encode(&self) -> Token {
        Token::Tuple(vec![
            self.l2_protocol_upgrade_tx.encode(),
            Token::Array(
                self.factory_deps
                    .iter()
                    .map(|b| Token::Bytes(b.clone()))
                    .collect(),
            ),
            Token::FixedBytes(self.bootloader_hash.into()),
            Token::FixedBytes(self.default_account_hash.into()),
            Token::Address(self.verifier),
            self.verifier_params.encode(),
            Token::Bytes(self.l1_contracts_upgrade_calldata.clone()),
            Token::Bytes(self.post_upgrade_calldata.clone()),
            Token::Uint(self.upgrade_timestamp),
            Token::Uint(self.new_protocol_version),
        ])
    }

    pub fn decode(token: Token) -> anyhow::Result<Self> {
        let tokens = token.into_tuple().context("not a tuple")?;
        anyhow::ensure!(tokens.len() == 10);
        let mut t = tokens.into_iter();
        let mut next = || t.next().unwrap();
        Ok(Self {
            l2_protocol_upgrade_tx: L2CanonicalTransaction::decode(next())
                .context("l2_protocol_upgrade_tx")?
                .into(),
            factory_deps: next()
                .into_array()
                .context("factory_deps")?
                .into_iter()
                .enumerate()
                .map(|(i, b)| b.into_bytes().context(i))
                .collect::<Result<_, _>>()
                .context("factory_deps")?,
            bootloader_hash: next()
                .into_fixed_bytes()
                .and_then(|b| b.try_into().ok())
                .context("bootloader_hash")?,
            default_account_hash: next()
                .into_fixed_bytes()
                .and_then(|b| b.try_into().ok())
                .context("default_account_hash")?,
            verifier: next().into_address().context("verifier")?,
            verifier_params: VerifierParams::decode(next()).context("verifier_params")?,
            l1_contracts_upgrade_calldata: next()
                .into_bytes()
                .context("l1_contracts_upgrade_calldata")?,
            post_upgrade_calldata: next().into_bytes().context("post_upgrade_calldata")?,
            upgrade_timestamp: next().into_uint().context("upgrade_timestamp")?,
            new_protocol_version: next().into_uint().context("new_protocol_version")?,
        })
    }
}

#[derive(Debug)]
pub enum Transaction {
    L1 {
        /// Hashed data.
        tx: Box<L2CanonicalTransaction>,
        /// txn contains a commitment to factory_deps.
        factory_deps: Vec<Vec<u8>>,
        /// Auxiliary data, not hashed.
        eth_block: u64,
        received_timestamp_ms: u64,
    },
    L2(Vec<u8>),
}

impl Transaction {
    pub fn hash(&self) -> anyhow::Result<H256> {
        Ok(match self {
            Self::L1 {
                tx, factory_deps, ..
            } => {
                // verify data integrity
                let factory_deps_hashes: Vec<_> = factory_deps
                    .iter()
                    .map(|b| h256_to_u256(hash_bytecode(b)))
                    .collect();
                anyhow::ensure!(tx.factory_deps == factory_deps_hashes);
                tx.hash()
            }
            Self::L2(raw) => TransactionRequest::from_bytes_unverified(raw)?.1,
        })
    }
}
