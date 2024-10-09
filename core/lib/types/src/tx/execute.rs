use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_utils::{bytecode::hash_bytecode, ZeroPrefixHexSerde};

use crate::{ethabi, Address, EIP712TypedStructure, StructBuilder, H256, U256};

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
        // TODO (SMA-1608): We should not re-implement the ABI parts in different places, instead have the ABI available
        //  from the `zksync_contracts` crate.
        static FUNCTION_SIGNATURE: Lazy<[u8; 4]> = Lazy::new(|| {
            ethabi::short_signature(
                "create",
                &[
                    ethabi::ParamType::FixedBytes(32),
                    ethabi::ParamType::FixedBytes(32),
                    ethabi::ParamType::Bytes,
                ],
            )
        });
        let params = ethabi::encode(&[
            ethabi::Token::FixedBytes(salt.as_bytes().to_vec()),
            ethabi::Token::FixedBytes(contract_hash.as_bytes().to_vec()),
            ethabi::Token::Bytes(constructor_input),
        ]);

        FUNCTION_SIGNATURE.iter().copied().chain(params).collect()
    }

    /// Creates an instance for deploying the specified bytecode without additional dependencies. If necessary,
    /// additional deps can be added to `Self.factory_deps` after this call.
    pub fn for_deploy(
        salt: H256,
        contract_bytecode: Vec<u8>,
        constructor_input: &[ethabi::Token],
    ) -> Self {
        let bytecode_hash = hash_bytecode(&contract_bytecode);
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
}
