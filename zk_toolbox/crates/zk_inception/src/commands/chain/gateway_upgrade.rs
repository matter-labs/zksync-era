use anyhow::Context;
use clap::{Parser, ValueEnum};
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    withdraw::ZKSProvider,
};
use config::{
    forge_interface::{
        gateway_chain_upgrade::{
            input::GatewayChainUpgradeInput, output::GatewayChainUpgradeOutput,
        },
        gateway_ecosystem_upgrade::output::GatewayEcosystemUpgradeOutput,
        gateway_preparation::{input::GatewayPreparationConfig, output::GatewayPreparationOutput},
        script_params::{GATEWAY_PREPARATION, GATEWAY_UPGRADE_CHAIN_PARAMS},
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use ethers::{
    abi::{encode, parse_abi},
    contract::BaseContract,
    providers::{Http, Middleware, Provider},
    types::Bytes,
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use types::L1BatchCommitmentMode;
use xshell::Shell;
use zksync_basic_types::{settlement::SettlementMode, H256, U256, U64};
use zksync_config::configs::{chain, eth_sender::PubdataSendingMode};
use zksync_types::{web3::keccak256, Address, L2ChainId, H160, L2_NATIVE_TOKEN_VAULT_ADDRESS};
use zksync_web3_decl::client::{Client, L2};

use crate::{
    accept_ownership::{admin_execute_upgrade, admin_schedule_upgrade, admin_update_validator, set_da_validator_pair},
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED},
    utils::forge::{check_the_balance, fill_forge_private_key},
};

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
pub enum GatewayChainUpgradeStage {
    // some config paaram
    AdaptConfig,

    // Does not require admin, still needs to be done to update configs, etc
    PrepareStage1,

    // Should be executed after Stage1 of the governance upgrade
    FinalizeStage1,

    // Mainly about changing configs
    FinalizeStage2,

    // For tests in case a chain missed the correct window for the upgrade
    // and needs to execute after Stage2
    KeepUpStage2,
}

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct GatewayUpgradeArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    chain_upgrade_stage: GatewayChainUpgradeStage,
}

// TODO: use a different script here (i.e. make it have a different file)
lazy_static! {
    static ref GATEWAY_PREPARATION_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function startMigrateChainFromGateway(address chainAdmin,address accessControlRestriction,uint256 chainId) public",
            "function finishMigrateChainFromGateway(uint256 migratingChainId,uint256 gatewayChainId,uint256 l2BatchNumber,uint256 l2MessageIndex,uint16 l2TxNumberInBatch,bytes memory message,bytes32[] memory merkleProof) public",
        ])
        .unwrap(),
    );
}

pub async fn run(args: GatewayUpgradeArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = EcosystemConfig::from_file(shell)?;

    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let l1_url = chain_config
        .get_secrets_config()?
        .l1
        .context(MSG_L1_SECRETS_MUST_BE_PRESENTED)?
        .l1_rpc_url
        .expose_str()
        .to_string();

    match args.chain_upgrade_stage {
        GatewayChainUpgradeStage::AdaptConfig => adapt_config(shell, chain_config).await,
        GatewayChainUpgradeStage::PrepareStage1 => {
            prepare_stage1(shell, args, ecosystem_config, chain_config, l1_url).await
        }
        GatewayChainUpgradeStage::FinalizeStage1 => {
            finalize_stage1(shell, args, ecosystem_config, chain_config, l1_url).await
        }
        GatewayChainUpgradeStage::FinalizeStage2 => {
            panic!("Not supported");
        }
        GatewayChainUpgradeStage::KeepUpStage2 => {
            panic!("Not supported");
        }
    }

    // // TODO: this has to be done as the final stage of the chain upgrade.
    // contracts_config.bridges.l1_nullifier_addr = Some(contracts_config.bridges.shared.l1_address);
    // contracts_config.bridges.shared.l1_address = gateway_ecosystem_preparation_output.deployed_addresses.bridges.shared_bridge_proxy_addr;
}

fn encode_ntv_asset_id(l1_chain_id: U256, addr: Address) -> H256 {
    let encoded_data = encode(&[
        ethers::abi::Token::Uint(l1_chain_id),
        ethers::abi::Token::Address(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        ethers::abi::Token::Address(addr),
    ]);

    H256(keccak256(&encoded_data))
}

async fn adapt_config(shell: &Shell, chain_config: ChainConfig) -> anyhow::Result<()> {
    println!("Adapting config");
    let mut contracts_config = chain_config.get_contracts_config()?;
    let genesis_config = chain_config.get_genesis_config()?;

    contracts_config.l2.legacy_shared_bridge_addr = contracts_config.bridges.shared.l2_address;
    contracts_config.l1.base_token_asset_id = Some(encode_ntv_asset_id(
        genesis_config.l1_chain_id.0.into(),
        contracts_config.l1.base_token_addr,
    ));

    contracts_config.save_with_base_path(shell, &chain_config.configs)?;
    println!("Done");

    Ok(())
}

async fn prepare_stage1(
    shell: &Shell,
    args: GatewayUpgradeArgs,
    ecosystem_config: EcosystemConfig,
    chain_config: ChainConfig,
    l1_url: String,
) -> anyhow::Result<()> {
    let chain_upgrade_config_path =
        GATEWAY_UPGRADE_CHAIN_PARAMS.input(&ecosystem_config.link_to_code);

    let gateway_upgrade_input = GatewayChainUpgradeInput::new(&chain_config);
    gateway_upgrade_input.save(shell, chain_upgrade_config_path.clone())?;

    let mut forge = Forge::new(&ecosystem_config.path_to_l1_foundry())
        .script(
            &GATEWAY_UPGRADE_CHAIN_PARAMS.script(),
            args.forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_url)
        .with_slow()
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        chain_config.get_wallets_config()?.governor_private_key(),
    )?;

    println!("Preparing the chain for the upgrade!");

    forge.run(shell)?;

    println!("done!");

    let chain_output = GatewayChainUpgradeOutput::read(
        shell,
        GATEWAY_UPGRADE_CHAIN_PARAMS.output(&ecosystem_config.link_to_code),
    )?;

    let gateway_ecosystem_preparation_output =
        GatewayEcosystemUpgradeOutput::read_with_base_path(shell, ecosystem_config.config)?;

    // No need to save it, we have enough for now

    let mut contracts_config = chain_config.get_contracts_config()?;

    contracts_config.user_facing_bridgehub =
        Some(contracts_config.ecosystem_contracts.bridgehub_proxy_addr);
    contracts_config.user_facing_diamond_proxy = Some(contracts_config.l1.diamond_proxy_addr);
    contracts_config
        .ecosystem_contracts
        .stm_deployment_tracker_proxy_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .bridgehub
            .ctm_deployment_tracker_proxy_addr,
    );
    // This is force deployment data for creating new contracts, not really relevant here tbh,
    contracts_config.ecosystem_contracts.force_deployments_data = Some(hex::encode(
        &gateway_ecosystem_preparation_output
            .contracts_config
            .force_deployments_data
            .0,
    ));
    contracts_config.ecosystem_contracts.native_token_vault_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .native_token_vault_addr,
    );
    contracts_config
        .ecosystem_contracts
        .l1_bytecodes_supplier_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .l1_bytecodes_supplier_addr,
    );
    contracts_config.l1.access_control_restriction_addr =
        Some(chain_output.access_control_restriction);
    contracts_config.l1.chain_admin_addr = chain_output.chain_admin_addr;

    // TODO: this field is probably not needed at all
    contracts_config.l1.chain_proxy_admin_addr = Some(H160::zero());

    contracts_config.l1.rollup_l1_da_validator_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .rollup_l1_da_validator_addr,
    );
    contracts_config.l1.validium_l1_da_validator_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .validium_l1_da_validator_addr,
    );

    let validum = chain_config
        .get_genesis_config()?
        .l1_batch_commit_data_generator_mode
        == L1BatchCommitmentMode::Validium;

    // We do not use chain output because IMHO we should delete it altogether from there
    contracts_config.l2.da_validator_addr = if !validum {
        Some(
            gateway_ecosystem_preparation_output
                .contracts_config
                .expected_rollup_l2_da_validator,
        )
    } else {
        Some(
            gateway_ecosystem_preparation_output
                .contracts_config
                .expected_validium_l2_da_validator,
        )
    };
    contracts_config.l2.l2_native_token_vault_proxy_addr = Some(L2_NATIVE_TOKEN_VAULT_ADDRESS);
    contracts_config.l2.legacy_shared_bridge_addr = contracts_config.bridges.shared.l2_address;

    contracts_config.save_with_base_path(shell, chain_config.configs)?;

    Ok(())
}

async fn finalize_stage1(
    shell: &Shell,
    args: GatewayUpgradeArgs,
    ecosystem_config: EcosystemConfig,
    chain_config: ChainConfig,
    l1_url: String,
) -> anyhow::Result<()> {
    println!("Finalizing stage1 of chain upgrade!");

    let mut contracts_config = chain_config.get_contracts_config()?;
    let gateway_ecosystem_preparation_output =
        GatewayEcosystemUpgradeOutput::read_with_base_path(shell, &ecosystem_config.config)?;

    let old_validator_timelock = contracts_config.l1.validator_timelock_addr;
    let new_validator_timelock = gateway_ecosystem_preparation_output
        .deployed_addresses
        .validator_timelock_addr;

    let validators = [
        chain_config.get_wallets_config()?.operator.address,
        chain_config.get_wallets_config()?.blob_operator.address,
    ];

    println!("Setting new validators!");
    // TODO: these can be done in a single operation
    for val in validators {
        admin_update_validator(
            shell,
            &ecosystem_config,
            &chain_config,
            old_validator_timelock,
            val,
            false,
            chain_config.get_wallets_config()?.governor_private_key(),
            &args.forge_args,
            l1_url.clone(),
        )
        .await?;

        admin_update_validator(
            shell,
            &ecosystem_config,
            &chain_config,
            new_validator_timelock,
            val,
            true,
            chain_config.get_wallets_config()?.governor_private_key(),
            &args.forge_args,
            l1_url.clone(),
        )
        .await?;
    }

    println!("Setting new validators done!");

    contracts_config.l1.validator_timelock_addr = gateway_ecosystem_preparation_output
        .deployed_addresses
        .validator_timelock_addr;

    admin_schedule_upgrade(
        shell,
        &ecosystem_config,
        &chain_config.get_contracts_config()?,
        // TODO: maybe not have it as a constant
        U256::from(0x1900000000 as u64),
        // We only do instant upgrades for now
        U256::zero(),
        chain_config.get_wallets_config()?.governor_private_key(),
        &args.forge_args,
        l1_url.clone(),
    )
    .await?;

    admin_execute_upgrade(
        shell,
        &ecosystem_config,
        &chain_config.get_contracts_config()?,
        chain_config.get_wallets_config()?.governor_private_key(),
        gateway_ecosystem_preparation_output
            .chain_upgrade_diamond_cut
            .0,
        &args.forge_args,
        l1_url.clone(),
    )
    .await?;

    let l1_da_validator_contract = if chain_config.get_genesis_config()?.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Rollup {
        ecosystem_config.get_contracts_config()?.l1.rollup_l1_da_validator_addr
    } else {
        ecosystem_config.get_contracts_config()?.l1.validium_l1_da_validator_addr
    }.context("l1 da validator")?;

    set_da_validator_pair(
        shell,
        &ecosystem_config,
        contracts_config.l1.chain_admin_addr,
        chain_config.get_wallets_config()?.governor_private_key(),
        contracts_config.l1.diamond_proxy_addr,
        l1_da_validator_contract,
        contracts_config.l2.da_validator_addr.context("l2_da_validator_addr")?,
        &args.forge_args,
        l1_url,
    ).await?;

    contracts_config.save_with_base_path(shell, chain_config.configs)?;

    println!("done!");

    Ok(())
}

// async fn await_for_tx_to_complete(
//     gateway_provider: &Provider<Http>,
//     hash: H256,
// ) -> anyhow::Result<()> {
//     println!("Waiting for transaction to complete...");
//     while Middleware::get_transaction_receipt(gateway_provider, hash)
//         .await?
//         .is_none()
//     {
//         tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
//     }

//     // We do not handle network errors
//     let receipt = Middleware::get_transaction_receipt(gateway_provider, hash)
//         .await?
//         .unwrap();

//     if receipt.status == Some(U64::from(1)) {
//         println!("Transaction completed successfully!");
//     } else {
//         panic!("Transaction failed!");
//     }

//     Ok(())
// }

// async fn await_for_withdrawal_to_finalize(
//     gateway_provider: &Client<L2>,
//     hash: H256,
// ) -> anyhow::Result<()> {
//     println!("Waiting for withdrawal to finalize...");
//     while gateway_provider.get_withdrawal_log(hash, 0).await.is_err() {
//         println!("Waiting for withdrawal to finalize...");
//         tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
//     }
//     Ok(())
// }

// async fn call_script(
//     shell: &Shell,
//     forge_args: ForgeScriptArgs,
//     data: &Bytes,
//     config: &EcosystemConfig,
//     private_key: Option<H256>,
//     rpc_url: String,
// ) -> anyhow::Result<H256> {
//     let mut forge = Forge::new(&config.path_to_l1_foundry())
//         .script(&GATEWAY_PREPARATION.script(), forge_args.clone())
//         .with_ffi()
//         .with_rpc_url(rpc_url)
//         .with_broadcast()
//         .with_calldata(data);

//     // Governor private key is required for this script
//     forge = fill_forge_private_key(forge, private_key)?;
//     check_the_balance(&forge).await?;
//     forge.run(shell)?;

//     let gateway_preparation_script_output =
//         GatewayPreparationOutput::read(shell, GATEWAY_PREPARATION.output(&config.link_to_code))?;

//     Ok(gateway_preparation_script_output.governance_l2_tx_hash)
// }
