use std::{num::NonZeroUsize, str::FromStr, sync::Arc};

use anyhow::Context;
use clap::{Parser, ValueEnum};
use ethers::{
    abi::{encode, parse_abi, Token},
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

#[derive(Debug, Clone, Serialize)]
struct AdminCall {
    description: String,
    target: Address,
    #[serde(serialize_with = "serialize_hex")]
    data: Vec<u8>,
    value: U256,
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
    validator_timelock_abi: BaseContract,
    gateway_transaction_filterer_abi: BaseContract,
    zkchain_abi: ethabi::Contract,
    chain_admin_abi: ethabi::Contract,
}

impl AdminCallBuilder {
    pub fn new() -> Self {
        Self {
            calls: vec![],
            validator_timelock_abi: BaseContract::from(
                parse_abi(&[
                    "function addValidator(uint256 _chainId, address _newValidator) external",
                ])
                .unwrap(),
            ),
            gateway_transaction_filterer_abi: BaseContract::from(
                parse_abi(&["function grantWhitelist(address sender) external"]).unwrap(),
            ),
            zkchain_abi: hyperchain_contract(),
            chain_admin_abi: chain_admin_contract(),
        }
    }

    pub fn append_validator(
        &mut self,
        chain_id: u64,
        validator_timelock_addr: Address,
        validator_addr: Address,
    ) -> Self {
        todo!()
        // let data = self
        //     .validator_timelock_abi
        //     .encode("addValidator", (U256::from(chain_id), validator_addr))
        //     .unwrap();
        // let description = format!(
        //     "Adding validator 0x{}",
        //     hex::encode(validator_timelock_addr.0)
        // );

        // let call = AdminCall {
        //     description,
        //     data: data.to_vec(),
        //     target: validator_timelock_addr,
        //     value: U256::zero(),
        // };

        // self.calls.push(call);

        // self
    }

    pub fn append_execute_upgrade(
        &mut self,
        hyperchain_addr: Address,
        protocol_version: u64,
        diamond_cut_data: Bytes,
    ) -> Self {
        todo!()
        // let diamond_cut = DIAMOND_CUT.decode_input(&diamond_cut_data.0).unwrap()[0].clone();

        // let data = self
        //     .zkchain_abi
        //     .function("upgradeChainFromVersion")
        //     .unwrap()
        //     .encode_input(&[Token::Uint(protocol_version.into()), diamond_cut])
        //     .unwrap();
        // let description = "Executing upgrade:".to_string();

        // let call = AdminCall {
        //     description,
        //     data: data.to_vec(),
        //     target: hyperchain_addr,
        //     value: U256::zero(),
        // };

        // self.calls.push(call);

        // self
    }

    pub fn append_set_da_validator_pair(
        &mut self,
        hyperchain_addr: Address,
        l1_da_validator: Address,
        l2_da_validator: Address,
    ) -> Self {
        todo!()
        // let data = self
        //     .zkchain_abi
        //     .function("setDAValidatorPair")
        //     .unwrap()
        //     .encode_input(&[
        //         Token::Address(l1_da_validator),
        //         Token::Address(l2_da_validator),
        //     ])
        //     .unwrap();
        // let description = "Setting DA validator pair".to_string();

        // let call = AdminCall {
        //     description,
        //     data: data.to_vec(),
        //     target: hyperchain_addr,
        //     value: U256::zero(),
        // };

        // self.calls.push(call);

        // self
    }

    pub fn append_make_permanent_rollup(&mut self, hyperchain_addr: Address) -> Self {
        todo!()
        // let data = self
        //     .zkchain_abi
        //     .function("makePermanentRollup")
        //     .unwrap()
        //     .encode_input(&[])
        //     .unwrap();
        // let description = "Make permanent rollup:".to_string();

        // let call = AdminCall {
        //     description,
        //     data: data.to_vec(),
        //     target: hyperchain_addr,
        //     value: U256::zero(),
        // };

        // self.calls.push(call);

        // self
    }

    pub fn append_grant_gateway_whitelist(
        &mut self,
        gateway_transaction_filterer: Address,
        grantee: Address,
    ) -> Self {
        todo!()
        // let data = self.gateway_transaction_filterer_abi.encode("grantWhitelist", (grantee)).unwrap();
        // let description = format!("Grant whitelist to {:#?}", grantee);

        // self.calls.push(AdminCall {
        //     description,
        //     target: gateway_transaction_filterer,
        //     data,
        //     value: U256::zero()
        // });

        // self
    }

    pub fn append_add_tx_filterer(
        &mut self,
        transaction_filterer: Address,
        zk_chain_address: Address,
    ) -> Self {
        todo!()
        // let data = self.zkchain_abi.function("setTransactionFilterer").unwrap().encode_input(&[Token::Address(transaction_filterer)]).unwrap();
        // let description = format!("Set transaction filterer to {:#?}", transaction_filterer);

        // self.calls.push(AdminCall {
        //     description,
        //     target: zk_chain_address,
        //     data,
        //     value: U256::zero()
        // });

        // self
    }

    pub fn to_json_string(&self) -> String {
        // Serialize with pretty printing
        serde_json::to_string_pretty(&self.calls).unwrap()
    }

    pub fn compile_full_calldata(self) -> Vec<u8> {
        let tokens: Vec<_> = self.calls.into_iter().map(|x| x.into_token()).collect();

        let data = self
            .chain_admin_abi
            .function("multicall")
            .unwrap()
            .encode_input(&[Token::Array(tokens), Token::Bool(true)])
            .unwrap();

        data.to_vec()
    }
}

pub(crate) fn get_ethers_provider(url: &str) -> anyhow::Result<Arc<Provider<Http>>> {
    let provider = match Provider::<Http>::try_from(url) {
        Ok(provider) => provider,
        Err(err) => {
            anyhow::bail!("Connection error: {:#?}", err);
        }
    };

    Ok(Arc::new(provider))
}

pub(crate) async fn broadcast_call(
    l1_rpc_url: String,
    private_key: String,
    to: Address,
    data: Vec<u8>,
) -> anyhow::Result<TransactionReceipt> {
    let ethers_provider = get_ethers_provider(&l1_rpc_url)?;

    let wallet = private_key.parse::<ethers::signers::LocalWallet>()?;
    let wallet = wallet.with_chain_id(ethers_provider.get_chainid().await?.as_u64());
    let client = ethers::middleware::SignerMiddleware::new(ethers_provider.clone(), wallet);

    let tx = TransactionRequest::new()
        .to(to)
        .data(data)
        .value(U256::zero());

    let pending_tx = client.send_transaction(tx, None).await?;

    // 5. Await receipt
    let Some(receipt) = pending_tx.await? else {
        anyhow::bail!("Failed to obtain the receipt for transaction");
    };

    Ok(receipt)
}
