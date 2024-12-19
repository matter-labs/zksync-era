use anyhow::Context;
use clap::{Parser, ValueEnum};
use common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
};
use config::{
    forge_interface::{
        gateway_chain_upgrade::{
            input::GatewayChainUpgradeInput, output::GatewayChainUpgradeOutput,
        },
        gateway_ecosystem_upgrade::output::GatewayEcosystemUpgradeOutput,
        script_params::{GATEWAY_UPGRADE_CHAIN_PARAMS, GATEWAY_UPGRADE_ECOSYSTEM_PARAMS},
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use ethers::{
    abi::{decode, encode, parse_abi, ParamType},
    contract::BaseContract,
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use types::L1BatchCommitmentMode;
use xshell::Shell;
use zksync_basic_types::{H256, U256};
use zksync_eth_client::EthInterface;
use zksync_types::{
    web3::{keccak256, CallRequest},
    Address, L2_NATIVE_TOKEN_VAULT_ADDRESS,
};
use zksync_web3_decl::client::{Client, L1};

use crate::{
    accept_ownership::{
        admin_execute_upgrade, admin_schedule_upgrade, admin_update_validator,
        set_da_validator_pair,
    },
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_L1_SECRETS_MUST_BE_PRESENTED},
    utils::forge::{fill_forge_private_key, WalletOwner},
};

lazy_static! {
    static ref ZK_CHAIN: BaseContract =
        BaseContract::from(parse_abi(&["function getPriorityTreeStartIndex() public",]).unwrap(),);
}

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
pub enum GatewayChainUpgradeStage {
    // some config paaram
    AdaptConfig,

    // Does not require admin, still needs to be done to update configs, etc
    PrepareStage1,

    // Used to schedule an upgrade.
    ScheduleStage1,

    // Should be executed after Stage1 of the governance upgrade
    FinalizeStage1,

    // Mainly about changing configs
    FinalizeStage2,

    // For tests in case a chain missed the correct window for the upgrade
    // and needs to execute after Stage2
    KeepUpStage2,

    // Set L2 WETH address for chain in store
    SetL2WETHForChain,
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
        GatewayChainUpgradeStage::ScheduleStage1 => {
            schedule_stage1(shell, args, ecosystem_config, chain_config, l1_url).await
        }
        GatewayChainUpgradeStage::FinalizeStage1 => {
            finalize_stage1(shell, args, ecosystem_config, chain_config, l1_url).await
        }
        GatewayChainUpgradeStage::FinalizeStage2 => {
            finalize_stage2(shell, ecosystem_config, chain_config).await
        }
        GatewayChainUpgradeStage::KeepUpStage2 => {
            panic!("Not supported");
        }
        GatewayChainUpgradeStage::SetL2WETHForChain => {
            set_weth_for_chain(shell, args, ecosystem_config, chain_config, l1_url).await
        }
    }
}

pub fn encode_ntv_asset_id(l1_chain_id: U256, addr: Address) -> H256 {
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
        Some(&chain_config.get_wallets_config()?.governor),
        WalletOwner::Governor,
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

    contracts_config.l1.rollup_l1_da_validator_addr = Some(
        gateway_ecosystem_preparation_output
            .deployed_addresses
            .rollup_l1_da_validator_addr,
    );
    contracts_config.l1.no_da_validium_l1_validator_addr = Some(
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

const NEW_PROTOCOL_VERSION: u64 = 0x1b00000000;

async fn schedule_stage1(
    shell: &Shell,
    args: GatewayUpgradeArgs,
    ecosystem_config: EcosystemConfig,
    chain_config: ChainConfig,
    l1_url: String,
) -> anyhow::Result<()> {
    println!("Schedule stage1 of the upgrade!!");

    admin_schedule_upgrade(
        shell,
        &ecosystem_config,
        &chain_config.get_contracts_config()?,
        // For now it is hardcoded both in scripts and here
        U256::from(NEW_PROTOCOL_VERSION),
        // We only do instant upgrades for now
        U256::zero(),
        &chain_config.get_wallets_config()?.governor,
        &args.forge_args,
        l1_url.clone(),
    )
    .await?;

    println!("done!");

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

    let mut geneal_config = chain_config.get_general_config()?;
    let genesis_config = chain_config.get_genesis_config()?;
    let mut contracts_config = chain_config.get_contracts_config()?;
    let secrets_config = chain_config.get_secrets_config()?;
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
    for val in validators {
        admin_update_validator(
            shell,
            &ecosystem_config,
            &chain_config,
            old_validator_timelock,
            val,
            false,
            &chain_config.get_wallets_config()?.governor,
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
            &chain_config.get_wallets_config()?.governor,
            &args.forge_args,
            l1_url.clone(),
        )
        .await?;
    }

    println!("Setting new validators done!");

    contracts_config.l1.validator_timelock_addr = gateway_ecosystem_preparation_output
        .deployed_addresses
        .validator_timelock_addr;

    admin_execute_upgrade(
        shell,
        &ecosystem_config,
        &chain_config.get_contracts_config()?,
        &chain_config.get_wallets_config()?.governor,
        gateway_ecosystem_preparation_output
            .chain_upgrade_diamond_cut
            .0,
        &args.forge_args,
        l1_url.clone(),
    )
    .await?;

    let l1_da_validator_contract = if chain_config
        .get_genesis_config()?
        .l1_batch_commit_data_generator_mode
        == L1BatchCommitmentMode::Rollup
    {
        ecosystem_config
            .get_contracts_config()?
            .l1
            .rollup_l1_da_validator_addr
    } else {
        ecosystem_config
            .get_contracts_config()?
            .l1
            .no_da_validium_l1_validator_addr
    }
    .context("l1 da validator")?;

    set_da_validator_pair(
        shell,
        &ecosystem_config,
        contracts_config.l1.chain_admin_addr,
        &chain_config.get_wallets_config()?.governor,
        contracts_config.l1.diamond_proxy_addr,
        l1_da_validator_contract,
        contracts_config
            .l2
            .da_validator_addr
            .context("l2_da_validator_addr")?,
        &args.forge_args,
        l1_url,
    )
    .await?;

    let client = Box::new(
        Client::<L1>::http(secrets_config.l1.clone().context("l1 secrets")?.l1_rpc_url)
            .context("Client::new()")?
            .for_network(genesis_config.l1_chain_id.into())
            .build(),
    );
    let request = CallRequest {
        to: Some(contracts_config.l1.diamond_proxy_addr),
        data: Some(
            zksync_types::ethabi::short_signature("getPriorityTreeStartIndex", &[])
                .to_vec()
                .into(),
        ),
        ..Default::default()
    };
    let result = client.call_contract_function(request, None).await?;

    let priority_tree_start_index = decode(&[ParamType::Uint(32)], &result.0)?
        .pop()
        .unwrap()
        .into_uint()
        .unwrap();

    geneal_config
        .eth
        .as_mut()
        .context("general_config_eth")?
        .sender
        .as_mut()
        .context("eth sender")?
        .priority_tree_start_index = Some(priority_tree_start_index.as_usize());

    contracts_config.save_with_base_path(shell, &chain_config.configs)?;
    geneal_config.save_with_base_path(shell, &chain_config.configs)?;

    println!("done!");

    Ok(())
}

async fn finalize_stage2(
    shell: &Shell,
    ecosystem_config: EcosystemConfig,
    chain_config: ChainConfig,
) -> anyhow::Result<()> {
    println!("Finalizing stage2 for the chain! (just updating configs)");

    let ecosystem_config = ecosystem_config.get_contracts_config()?;

    let mut contracts_config = chain_config.get_contracts_config()?;
    contracts_config.bridges.l1_nullifier_addr = Some(contracts_config.bridges.shared.l1_address);
    contracts_config.bridges.shared.l1_address = ecosystem_config.bridges.shared.l1_address;
    contracts_config.bridges.shared.l2_address =
        Some(zksync_system_constants::L2_ASSET_ROUTER_ADDRESS);
    contracts_config.save_with_base_path(shell, &chain_config.configs)?;

    println!("done!");

    Ok(())
}

async fn set_weth_for_chain(
    shell: &Shell,
    args: GatewayUpgradeArgs,
    ecosystem_config: EcosystemConfig,
    chain_config: ChainConfig,
    l1_url: String,
) -> anyhow::Result<()> {
    println!("Adding l2 weth to store!");

    let forge_args = args.forge_args.clone();
    let l1_rpc_url = l1_url;

    let previous_output = GatewayEcosystemUpgradeOutput::read(
        shell,
        GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.output(&ecosystem_config.link_to_code),
    )?;
    let contract: BaseContract = BaseContract::from(
        parse_abi(&[
            "function addL2WethToStore(address storeAddress, address chainAdmin, uint256 chainId, address l2WBaseToken) public",
        ])
            .unwrap(),
    );
    let contracts_config = chain_config.get_contracts_config()?;
    let calldata = contract
        .encode(
            "addL2WethToStore",
            (
                previous_output
                    .deployed_addresses
                    .l2_wrapped_base_token_store_addr,
                ecosystem_config
                    .get_contracts_config()
                    .expect("get_contracts_config()")
                    .l1
                    .chain_admin_addr,
                chain_config.chain_id.0,
                contracts_config
                    .l2
                    .predeployed_l2_wrapped_base_token_address
                    .expect("No predeployed_l2_wrapped_base_token_address"),
            ),
        )
        .unwrap();

    let mut forge = Forge::new(&ecosystem_config.path_to_l1_foundry())
        .script(
            &GATEWAY_UPGRADE_ECOSYSTEM_PARAMS.script(),
            forge_args.clone(),
        )
        .with_ffi()
        .with_rpc_url(l1_rpc_url)
        .with_slow()
        .with_gas_limit(1_000_000_000_000)
        .with_calldata(&calldata)
        .with_broadcast();

    forge = fill_forge_private_key(
        forge,
        Some(&ecosystem_config.get_wallets()?.governor),
        WalletOwner::Governor,
    )?;
    forge.run(shell)?;

    Ok(())
}
