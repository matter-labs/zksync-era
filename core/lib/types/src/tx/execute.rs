use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{address_to_h256, bytecode::BytecodeHash, web3::keccak256};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;

use crate::{
    ethabi, serde_wrappers::ZeroPrefixHexSerde, Address, EIP712TypedStructure, StructBuilder, H256,
    U256,
};

/// This struct is the `serde` schema for the `Execute` struct.
/// It allows us to modify `Execute` struct without worrying
/// about encoding compatibility.
///
/// For example, changing type of `factory_deps` from `Option<Vec<Vec<u8>>`
/// to `Vec<Vec<u8>>` (even with `#[serde(default)]` annotation)
/// would be incompatible for `serde` json encoding,
/// because `null` is a valid value for the former but not for the latter.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteSerde {
    contract_address: Option<Address>,
    #[serde(with = "ZeroPrefixHexSerde")]
    calldata: Vec<u8>,
    value: U256,
    factory_deps: Option<Vec<Vec<u8>>>,
}

/// `Execute` transaction executes a previously deployed smart contract in the L2 rollup.
#[derive(Clone, Default, PartialEq)]
pub struct Execute {
    pub contract_address: Option<Address>,
    pub calldata: Vec<u8>,
    pub value: U256,
    /// Factory dependencies: list of contract bytecodes associated with the deploy transaction.
    pub factory_deps: Vec<Vec<u8>>,
}

impl serde::Serialize for Execute {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ExecuteSerde {
            contract_address: self.contract_address,
            calldata: self.calldata.clone(),
            value: self.value,
            factory_deps: Some(self.factory_deps.clone()),
        }
        .serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for Execute {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let x = ExecuteSerde::deserialize(d)?;
        Ok(Self {
            contract_address: x.contract_address,
            calldata: x.calldata,
            value: x.value,
            factory_deps: x.factory_deps.unwrap_or_default(),
        })
    }
}

impl std::fmt::Debug for Execute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let factory_deps = format!("<{} factory deps>", self.factory_deps.len());
        f.debug_struct("Execute")
            .field("contract_address", &self.contract_address)
            .field("calldata", &hex::encode(&self.calldata))
            .field("value", &self.value)
            .field("factory_deps", &factory_deps)
            .finish()
    }
}

impl EIP712TypedStructure for Execute {
    const TYPE_NAME: &'static str = "Transaction";

    fn build_structure<BUILDER: StructBuilder>(&self, builder: &mut BUILDER) {
        if let Some(contract_address) = self.contract_address {
            builder.add_member("to", &contract_address);
        }
        builder.add_member("value", &self.value);
        builder.add_member("data", &self.calldata.as_slice());
        // Factory deps are not included into the transaction signature, since they are parsed from the
        // transaction metadata.
        // Note that for the deploy transactions all the dependencies are implicitly included into the `calldataHash`
        // field, because the deps are referenced in the bytecode of the "main" contract bytecode.
    }
}

const CREATE_PARAMS: &[ethabi::ParamType] = &[
    ethabi::ParamType::FixedBytes(32),
    ethabi::ParamType::FixedBytes(32),
    ethabi::ParamType::Bytes,
];
const CREATE_ACC_PARAMS: &[ethabi::ParamType] = &[
    ethabi::ParamType::FixedBytes(32),
    ethabi::ParamType::FixedBytes(32),
    ethabi::ParamType::Bytes,
    ethabi::ParamType::Uint(8),
];

// TODO (SMA-1608): We should not re-implement the ABI parts in different places, instead have the ABI available
//  from the `zksync_contracts` crate.
static CREATE_FUNCTION: Lazy<[u8; 4]> =
    Lazy::new(|| ethabi::short_signature("create", CREATE_PARAMS));
static CREATE2_FUNCTION: Lazy<[u8; 4]> =
    Lazy::new(|| ethabi::short_signature("create2", CREATE_PARAMS));
static CREATE_ACC_FUNCTION: Lazy<[u8; 4]> =
    Lazy::new(|| ethabi::short_signature("createAccount", CREATE_ACC_PARAMS));
static CREATE2_ACC_FUNCTION: Lazy<[u8; 4]> =
    Lazy::new(|| ethabi::short_signature("create2Account", CREATE_ACC_PARAMS));

impl Execute {
    pub fn calldata(&self) -> &[u8] {
        &self.calldata
    }

    /// Prepares calldata to invoke deployer contract. This method encodes parameters for the `create` method.
    pub fn encode_deploy_params_create(
        salt: H256,
        contract_hash: H256,
        constructor_input: Vec<u8>,
    ) -> Vec<u8> {
        let params = ethabi::encode(&[
            ethabi::Token::FixedBytes(salt.as_bytes().to_vec()),
            ethabi::Token::FixedBytes(contract_hash.as_bytes().to_vec()),
            ethabi::Token::Bytes(constructor_input),
        ]);
        CREATE_FUNCTION.iter().copied().chain(params).collect()
    }

    /// Creates an instance for deploying the specified bytecode without additional dependencies. If necessary,
    /// additional deps can be added to `Self.factory_deps` after this call.
    pub fn for_deploy(
        salt: H256,
        contract_bytecode: Vec<u8>,
        constructor_input: &[ethabi::Token],
    ) -> Self {
        let bytecode_hash = BytecodeHash::for_bytecode(&contract_bytecode).value();
        Self {
            contract_address: Some(CONTRACT_DEPLOYER_ADDRESS),
            calldata: Self::encode_deploy_params_create(
                salt,
                bytecode_hash,
                ethabi::encode(constructor_input),
            ),
            value: 0.into(),
            factory_deps: vec![contract_bytecode],
        }
    }

    /// Creates an instance for deploying the specified bytecode via `ContractDeployer.create2` without additional dependencies.
    /// If necessary, additional deps can be added to `Self.factory_deps` after this call.
    pub fn for_create2_deploy(
        salt: H256,
        contract_bytecode: Vec<u8>,
        constructor_input: &[ethabi::Token],
    ) -> (Self, Create2DeploymentParams) {
        let bytecode_hash = BytecodeHash::for_bytecode(&contract_bytecode).value();
        let raw_constructor_input = ethabi::encode(constructor_input);
        let params = ethabi::encode(&[
            ethabi::Token::FixedBytes(salt.as_bytes().to_vec()),
            ethabi::Token::FixedBytes(bytecode_hash.as_bytes().to_vec()),
            ethabi::Token::Bytes(raw_constructor_input.clone()),
        ]);
        let calldata = CREATE2_FUNCTION.iter().copied().chain(params).collect();
        let execute = Self {
            contract_address: Some(CONTRACT_DEPLOYER_ADDRESS),
            calldata,
            value: 0.into(),
            factory_deps: vec![contract_bytecode],
        };

        let deployment_params = Create2DeploymentParams {
            salt,
            bytecode_hash,
            raw_constructor_input,
        };
        (execute, deployment_params)
    }

    /// Creates an instance for transferring base token to the specified recipient.
    pub fn transfer(to: Address, value: U256) -> Self {
        Self {
            contract_address: Some(to),
            calldata: vec![],
            value,
            factory_deps: vec![],
        }
    }
}

/// Deployment params for a `ContractDeployer.{create2, create2Account}` call.
#[derive(Debug)]
pub struct Create2DeploymentParams {
    pub salt: H256,
    pub bytecode_hash: H256,
    pub raw_constructor_input: Vec<u8>,
}

impl Create2DeploymentParams {
    /// Assumes tokens have expected shape.
    fn from_tokens(tokens: Vec<ethabi::Token>) -> Self {
        let mut tokens = tokens.into_iter();
        // Salt is the first token. `unwrap()`s are safe because of the successful decoding.
        let salt = tokens.next().unwrap();
        let salt = H256::from_slice(&salt.into_fixed_bytes().unwrap());
        // Bytecode hash is the second token.
        let bytecode_hash = tokens.next().unwrap();
        let bytecode_hash = H256::from_slice(&bytecode_hash.into_fixed_bytes().unwrap());
        // Raw constructor input is the 3rd token.
        let raw_constructor_input = tokens.next().unwrap();
        let raw_constructor_input = raw_constructor_input.into_bytes().unwrap();

        Self {
            salt,
            bytecode_hash,
            raw_constructor_input,
        }
    }

    /// Pre-calculates the address of the to-be-deployed EraVM contract (via CREATE2).
    pub fn derive_address(&self, sender: Address) -> Address {
        let prefix_bytes = keccak256("zksyncCreate2".as_bytes());
        let address_bytes = address_to_h256(&sender);

        let mut bytes = [0u8; 160];
        bytes[..32].copy_from_slice(&prefix_bytes);
        bytes[32..64].copy_from_slice(address_bytes.as_bytes());
        bytes[64..96].copy_from_slice(self.salt.as_bytes());
        bytes[96..128].copy_from_slice(self.bytecode_hash.as_bytes());
        bytes[128..].copy_from_slice(&keccak256(&self.raw_constructor_input));

        Address::from_slice(&keccak256(&bytes)[12..])
    }
}

/// Deployment params encoding various canonical ways to deploy EraVM contracts.
#[derive(Debug)]
pub enum DeploymentParams {
    Create,
    Create2(Create2DeploymentParams),
    CreateAccount,
    Create2Account(Create2DeploymentParams),
}

impl DeploymentParams {
    /// Returns `None` if calldata doesn't correspond to the 4 deploying `ContractDeployer` methods,
    /// or if it cannot be decoded.
    pub fn decode(calldata: &[u8]) -> Option<Self> {
        if calldata.len() < 4 {
            return None;
        }
        let (short_signature, token_data) = calldata.split_at(4);
        match short_signature {
            sig if sig == *CREATE_FUNCTION => {
                ethabi::decode(CREATE_PARAMS, token_data).ok()?;
                Some(Self::Create)
            }
            sig if sig == *CREATE2_FUNCTION => {
                let tokens = ethabi::decode(CREATE_PARAMS, token_data).ok()?;
                Some(Self::Create2(Create2DeploymentParams::from_tokens(tokens)))
            }
            sig if sig == *CREATE_ACC_FUNCTION => {
                ethabi::decode(CREATE_ACC_PARAMS, token_data).ok()?;
                Some(Self::CreateAccount)
            }
            sig if sig == *CREATE2_ACC_FUNCTION => {
                let tokens = ethabi::decode(CREATE_ACC_PARAMS, token_data).ok()?;
                Some(Self::Create2Account(Create2DeploymentParams::from_tokens(
                    tokens,
                )))
            }
            _ => None,
        }
    }
}
