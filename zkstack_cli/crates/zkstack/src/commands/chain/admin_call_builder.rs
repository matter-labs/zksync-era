use std::{num::NonZeroUsize, str::FromStr, sync::Arc};

use anyhow::Context;
use clap::{Parser, ValueEnum};
use ethers::{
    abi::{decode, encode, parse_abi, ParamType, Token},
    contract::{abigen, BaseContract},
    providers::{Http, Middleware, Provider},
    signers::Signer,
    types::{TransactionReceipt, TransactionRequest},
    utils::hex,
};
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use xshell::Shell;
use zkstack_cli_config::{
    forge_interface::gateway_ecosystem_upgrade::output::GatewayEcosystemUpgradeOutput,
    traits::{ReadConfig, ZkStackConfig},
    ContractsConfig,
};
use zksync_contracts::{chain_admin_contract, hyperchain_contract, DIAMOND_CUT};
use zksync_types::{
    address_to_h256, ethabi, h256_to_address,
    url::SensitiveUrl,
    web3::{keccak256, Bytes},
    Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId, CONTRACT_DEPLOYER_ADDRESS,
    H256, L2_NATIVE_TOKEN_VAULT_ADDRESS, U256,
};
use zksync_web3_decl::{
    client::{Client, DynClient, L2},
    namespaces::{EthNamespaceClient, UnstableNamespaceClient, ZksNamespaceClient},
};

use super::utils::get_ethers_provider;

#[derive(Debug, Clone, Serialize)]
pub struct AdminCall {
    description: String,
    target: Address,
    #[serde(serialize_with = "serialize_hex")]
    data: Vec<u8>,
    value: U256,
}

pub(crate) fn decode_admin_calls(encoded_calls: &[u8]) -> anyhow::Result<Vec<AdminCall>> {
    let calls = decode(
        &[ParamType::Array(Box::new(ParamType::Tuple(vec![
            ParamType::Address,
            ParamType::Uint(256),
            ParamType::Bytes,
        ])))],
        encoded_calls,
    )?
    .pop()
    .unwrap()
    .into_array()
    .unwrap();

    let calls = calls
        .into_iter()
        .map(|call| {
            // The type was checked during decoding, so "unwrap" is safe
            let subfields = call.into_tuple().unwrap();

            AdminCall {
                // TODO: For now, only empty descriptions are available
                description: "".into(),
                // The type was checked during decoding, so "unwrap" is safe
                target: subfields[0].clone().into_address().unwrap(),
                // The type was checked during decoding, so "unwrap" is safe
                value: subfields[1].clone().into_uint().unwrap(),
                // The type was checked during decoding, so "unwrap" is safe
                data: subfields[2].clone().into_bytes().unwrap(),
            }
        })
        .collect();

    Ok(calls)
}

impl AdminCall {
    fn into_token(self) -> Token {
        let Self {
            target,
            data,
            value,
            ..
        } = self;
        Token::Tuple(vec![
            Token::Address(target),
            Token::Uint(value),
            Token::Bytes(data),
        ])
    }
}

fn hex_address_display(addr: Address) -> String {
    format!("0x{}", hex::encode(addr.0))
}

fn serialize_hex<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let hex_string = format!("0x{}", hex::encode(bytes));
    serializer.serialize_str(&hex_string)
}

#[derive(Debug, Clone)]
pub struct AdminCallBuilder {
    calls: Vec<AdminCall>,
    zkchain_abi: ethabi::Contract,
    chain_admin_abi: ethabi::Contract,
}

impl AdminCallBuilder {
    pub fn new(calls: Vec<AdminCall>) -> Self {
        Self {
            calls: calls,
            zkchain_abi: hyperchain_contract(),
            chain_admin_abi: chain_admin_contract(),
        }
    }

    #[cfg(feature = "v27_evm_interpreter")]
    pub fn append_execute_upgrade(
        &mut self,
        hyperchain_addr: Address,
        protocol_version: u64,
        diamond_cut_data: Bytes,
    ) {
        let diamond_cut = DIAMOND_CUT.decode_input(&diamond_cut_data.0).unwrap()[0].clone();

        let data = self
            .zkchain_abi
            .function("upgradeChainFromVersion")
            .unwrap()
            .encode_input(&[Token::Uint(protocol_version.into()), diamond_cut])
            .unwrap();
        let description = "Executing upgrade:".to_string();

        let call = AdminCall {
            description,
            data: data.to_vec(),
            target: hyperchain_addr,
            value: U256::zero(),
        };

        self.calls.push(call);
    }

    pub fn to_json_string(&self) -> String {
        // Serialize with pretty printing
        serde_json::to_string_pretty(&self.calls).unwrap()
    }

    pub fn display(&self) {
        // Serialize with pretty printing
        let serialized = serde_json::to_string_pretty(&self.calls).unwrap();

        // Output the serialized JSON
        println!("{}", serialized);
    }

    pub fn compile_full_calldata(self) -> (Vec<u8>, U256) {
        let mut sum = U256::zero();
        let mut tokens = vec![];

        for call in self.calls {
            sum += call.value;
            tokens.push(call.into_token());
        }

        let data = self
            .chain_admin_abi
            .function("multicall")
            .unwrap()
            .encode_input(&[Token::Array(tokens), Token::Bool(true)])
            .unwrap();

        (data.to_vec(), sum)
    }
}
