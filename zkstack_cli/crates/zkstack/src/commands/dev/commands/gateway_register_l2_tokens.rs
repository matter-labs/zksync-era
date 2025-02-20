use std::{num::NonZeroUsize, str::FromStr, sync::Arc};

use anyhow::Context;
use clap::{Parser, ValueEnum};
use ethers::{
    abi::{encode, parse_abi, Token},
    contract::{abigen, BaseContract},
    providers::{Http, Middleware, Provider},
    signers::Signer,
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
    ethabi,
    url::SensitiveUrl,
    web3::{keccak256, Bytes},
    Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId, H256,
    L2_NATIVE_TOKEN_VAULT_ADDRESS, U256,
};
use zksync_web3_decl::{
    client::{Client, DynClient, L2},
    namespaces::{EthNamespaceClient, UnstableNamespaceClient, ZksNamespaceClient},
};

use super::gateway::{check_l2_ntv_existence, get_all_tokens, get_ethers_provider, get_zk_client};

// L2WrappedBaseTokenStore ABI
abigen!(
    L2NativeTokenVaultAbi,
    r"[
    function assetId(address)(bytes32)
    function setLegacyTokenAssetId(address _l2TokenAddress) public
    function L2_LEGACY_SHARED_BRIDGE()(address)
]"
);

abigen!(
    L2LegacySharedBridgeAbi,
    r"[
    function l1TokenAddress(address)(address)
]"
);

pub async fn migrate_l2_tokens(
    private_key: String,
    l2_rpc_url: String,
    l2_chain_id: u64,
) -> anyhow::Result<()> {
    let l2_client = get_zk_client(&l2_rpc_url, l2_chain_id)?;

    check_l2_ntv_existence(&l2_client).await?;
    let ethers_provider = get_ethers_provider(l2_rpc_url)?;

    let wallet = private_key.parse::<ethers::signers::LocalWallet>()?;
    let wallet = wallet.with_chain_id(l2_chain_id);
    let middleware = ethers::middleware::SignerMiddleware::new(ethers_provider.clone(), wallet);

    let l2_native_token_vault =
        L2NativeTokenVaultAbi::new(L2_NATIVE_TOKEN_VAULT_ADDRESS, Arc::new(middleware));
    let l2_legacy_shared_bridge_addr = l2_native_token_vault.l2_legacy_shared_bridge().await?;
    if l2_legacy_shared_bridge_addr == Address::zero() {
        println!("Chain does not have a legacy bridge. Nothing to migrate");
        return Ok(());
    }

    let l2_legacy_shared_bridge =
        L2LegacySharedBridgeAbi::new(l2_legacy_shared_bridge_addr, ethers_provider);

    let all_tokens = get_all_tokens(&l2_client).await?;

    for token in all_tokens {
        let current_asset_id = l2_native_token_vault.asset_id(token.l2_address).await?;
        // Let's double check whether the token can be registered at all
        let l1_address = l2_legacy_shared_bridge
            .l_1_token_address(token.l2_address)
            .await?;

        if current_asset_id == [0u8; 32] && l1_address != Address::zero() {
            println!(
                "Token {} (address {:#?}) is not registered. Registering...",
                token.name, token.l2_address
            );

            let call = l2_native_token_vault
                .method::<_, Address>("setLegacyTokenAssetId", token.l2_address)?;
            let pending_tx = call.send().await?;

            let receipt = pending_tx.await?;

            if let Some(receipt) = receipt {
                println!("Transaction included in block: {:?}", receipt.block_number);
            } else {
                anyhow::bail!("Transaction failed or was dropped.");
            }
        }
    }

    Ok(())
}

#[derive(Parser, Debug, Clone)]
pub struct GatewayRegisterL2TokensArgs {
    chain_id: u64,
    l2_rpc_url: String,
    private_key: String,
}

pub(crate) async fn run(args: GatewayRegisterL2TokensArgs) -> anyhow::Result<()> {
    println!("Looking for unregistered tokens...");

    migrate_l2_tokens(args.private_key, args.l2_rpc_url, args.chain_id).await?;

    println!("All tokens registered!");

    Ok(())
}
