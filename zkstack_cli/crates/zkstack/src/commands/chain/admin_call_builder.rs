use std::path::Path;

use ethers::{
    abi::{decode, ParamType, Token},
    types::Bytes,
    utils::hex,
};
use serde::Serialize;
use xshell::Shell;
use zkstack_cli_common::forge::ForgeScriptArgs;
use zksync_contracts::chain_admin_contract;
use zksync_types::{ethabi, Address, U256};

#[derive(Debug, Clone, Serialize)]
pub struct AdminCall {
    pub description: String,
    pub target: Address,
    #[serde(serialize_with = "serialize_hex")]
    pub data: Vec<u8>,
    pub value: U256,
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
                // TODO(EVM-999): For now, only empty descriptions are available
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
    chain_admin_abi: ethabi::Contract,
}

impl AdminCallBuilder {
    pub fn new(calls: Vec<AdminCall>) -> Self {
        Self {
            calls,
            chain_admin_abi: chain_admin_contract(),
        }
    }

    pub fn extend_with_calls(&mut self, calls: Vec<AdminCall>) {
        self.calls.extend(calls);
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_upgrade_chain_on_gateway_calls(
        &mut self,
        shell: &Shell,
        forge_args: &ForgeScriptArgs,
        foundry_contracts_path: &Path,
        chain_id: u64,
        gateway_chain_id: u64,
        bridgehub: Address,
        l1_gas_price: u64,
        old_protocol_version: u64,
        chain_diamond_proxy_on_gateway: Address,
        l1_asset_router_proxy: Address,
        refund_recipient: Address,
        upgrade_cut_data: Bytes,
        l1_rpc_url: String,
    ) {
        let result = crate::admin_functions::prepare_upgrade_zk_chain_on_gateway(
            shell,
            forge_args,
            foundry_contracts_path,
            crate::admin_functions::AdminScriptMode::OnlySave,
            chain_id,
            gateway_chain_id,
            bridgehub,
            l1_gas_price,
            old_protocol_version,
            chain_diamond_proxy_on_gateway,
            l1_asset_router_proxy,
            refund_recipient,
            upgrade_cut_data,
            l1_rpc_url,
        )
        .await;

        match result {
            Ok(output) => {
                self.calls.extend(output.calls);
            }
            Err(e) => {
                eprintln!("Error preparing upgrade: {}", e);
            }
        }
    }

    pub fn append_execute_upgrade(
        &mut self,
        hyperchain_addr: Address,
        protocol_version: u64,
        diamond_cut_data: zksync_types::web3::Bytes,
    ) {
        let diamond_cut = zksync_contracts::DIAMOND_CUT
            .decode_input(&diamond_cut_data.0)
            .unwrap()[0]
            .clone();
        let zkchain_abi = zksync_contracts::hyperchain_contract();

        let data = zkchain_abi
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
