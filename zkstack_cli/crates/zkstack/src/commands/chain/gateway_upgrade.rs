use anyhow::Context;
use clap::{Parser, ValueEnum};
use ethers::{
    abi::parse_abi,
    contract::BaseContract,
    providers::{Http, Middleware, Provider},
    signers::Signer,
    types::{transaction::eip2718::TypedTransaction, Eip1559TransactionRequest},
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
};
use zkstack_cli_config::{
    forge_interface::{
        gateway_ecosystem_upgrade::output::GatewayEcosystemUpgradeOutput,
        script_params::{ACCEPT_GOVERNANCE_SCRIPT_PARAMS, GATEWAY_UPGRADE_ECOSYSTEM_PARAMS},
    },
    traits::{ReadConfig, ReadConfigWithBasePath, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::U256;
use zksync_types::Address;

use crate::{
    commands::dev::commands::gateway::{
        check_chain_readiness, fetch_chain_info, get_admin_call_builder,
        set_upgrade_timestamp_calldata, DAMode, GatewayUpgradeArgsInner, GatewayUpgradeInfo,
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

    l2_wrapped_base_token_addr: Option<Address>,
}

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
        GatewayChainUpgradeStage::PrepareStage1 => {
            prepare_stage1(shell, ecosystem_config, chain_config, l1_url).await
        }
        GatewayChainUpgradeStage::ScheduleStage1 => {
            schedule_stage1(shell, ecosystem_config, chain_config, l1_url).await
        }
        GatewayChainUpgradeStage::FinalizeStage1 => {
            finalize_stage1(shell, ecosystem_config, chain_config, l1_url).await
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

async fn prepare_stage1(
    shell: &Shell,
    ecosystem_config: EcosystemConfig,
    chain_config: ChainConfig,
    l1_url: String,
) -> anyhow::Result<()> {
    let gateway_ecosystem_preparation_output =
        GatewayEcosystemUpgradeOutput::read_with_base_path(shell, ecosystem_config.config)?;

    // No need to save it, we have enough for now

    let mut contracts_config = chain_config.get_contracts_config()?;
    let general_config = chain_config.get_general_config()?;
    let genesis_config = chain_config.get_genesis_config()?;

    let upgrade_info = GatewayUpgradeInfo::from_gateway_ecosystem_upgrade(
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        gateway_ecosystem_preparation_output,
    );

    let da_mode: DAMode =
        if genesis_config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Rollup {
            DAMode::PermanentRollup
        } else {
            DAMode::Validium
        };

    let chain_info = fetch_chain_info(
        &upgrade_info,
        &GatewayUpgradeArgsInner {
            chain_id: chain_config.chain_id.as_u64(),
            l1_rpc_url: l1_url,
            l2_rpc_url: general_config
                .api_config
                .context("api config")?
                .web3_json_rpc
                .http_url,
            validator_addr1: chain_config.get_wallets_config()?.operator.address,
            validator_addr2: chain_config.get_wallets_config()?.blob_operator.address,
            da_mode,
            dangerous_no_cross_check: false,
        },
    )
    .await?;

    upgrade_info.update_contracts_config(&mut contracts_config, &chain_info, da_mode, true);

    contracts_config.save_with_base_path(shell, chain_config.configs)?;

    Ok(())
}

async fn call_chain_admin(
    l1_url: String,
    chain_config: ChainConfig,
    data: Vec<u8>,
) -> anyhow::Result<()> {
    let wallet = chain_config
        .get_wallets_config()?
        .governor
        .private_key
        .context("gov pk missing")?;
    let contracts_config = chain_config.get_contracts_config()?;

    // Initialize provider
    let provider = Provider::<Http>::try_from(l1_url)?;

    // Initialize wallet
    let chain_id = provider.get_chainid().await?.as_u64();
    let wallet = wallet.with_chain_id(chain_id);

    let tx = TypedTransaction::Eip1559(Eip1559TransactionRequest {
        to: Some(contracts_config.l1.chain_admin_addr.into()),
        // 10m should be always enough
        gas: Some(U256::from(10_000_000)),
        data: Some(data.into()),
        value: Some(U256::zero()),
        nonce: Some(
            provider
                .get_transaction_count(wallet.address(), None)
                .await?,
        ),
        max_fee_per_gas: Some(provider.get_gas_price().await?),
        max_priority_fee_per_gas: Some(U256::zero()),
        chain_id: Some(chain_id.into()),
        ..Default::default()
    });

    let signed_tx = wallet.sign_transaction(&tx).await.unwrap();

    let tx = provider
        .send_raw_transaction(tx.rlp_signed(&signed_tx))
        .await
        .unwrap();
    println!("Sent tx with hash: {}", hex::encode(tx.0));

    let receipt = tx.await?.context("receipt not present")?;

    if receipt.status.unwrap() != 1.into() {
        anyhow::bail!("Transaction failed!");
    }

    Ok(())
}

async fn schedule_stage1(
    shell: &Shell,
    ecosystem_config: EcosystemConfig,
    chain_config: ChainConfig,
    l1_url: String,
) -> anyhow::Result<()> {
    let gateway_ecosystem_preparation_output =
        GatewayEcosystemUpgradeOutput::read_with_base_path(shell, ecosystem_config.config)?;

    println!("Schedule stage1 of the upgrade!!");
    let calldata = set_upgrade_timestamp_calldata(
        gateway_ecosystem_preparation_output
            .contracts_config
            .new_protocol_version,
        // Immediatelly
        0,
    );

    call_chain_admin(l1_url, chain_config, calldata).await?;

    println!("done!");

    Ok(())
}

async fn finalize_stage1(
    shell: &Shell,
    ecosystem_config: EcosystemConfig,
    chain_config: ChainConfig,
    l1_url: String,
) -> anyhow::Result<()> {
    println!("Finalizing stage1 of chain upgrade!");

    let contracts_config = chain_config.get_contracts_config()?;
    let general_config = chain_config.get_general_config()?;
    let genesis_config = chain_config.get_genesis_config()?;

    println!("Checking chain readiness...");
    check_chain_readiness(
        l1_url.clone(),
        general_config
            .api_config
            .as_ref()
            .context("api")?
            .web3_json_rpc
            .http_url
            .clone(),
        chain_config.chain_id.as_u64(),
    )
    .await?;

    println!("The chain is ready!");

    let gateway_ecosystem_preparation_output =
        GatewayEcosystemUpgradeOutput::read_with_base_path(shell, &ecosystem_config.config)?;

    let da_mode: DAMode =
        if genesis_config.l1_batch_commit_data_generator_mode == L1BatchCommitmentMode::Rollup {
            DAMode::PermanentRollup
        } else {
            DAMode::Validium
        };

    let upgrade_info = GatewayUpgradeInfo::from_gateway_ecosystem_upgrade(
        contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
        gateway_ecosystem_preparation_output,
    );
    let args = GatewayUpgradeArgsInner {
        chain_id: chain_config.chain_id.as_u64(),
        l1_rpc_url: l1_url.clone(),
        l2_rpc_url: general_config
            .api_config
            .context("api config")?
            .web3_json_rpc
            .http_url,
        validator_addr1: chain_config.get_wallets_config()?.operator.address,
        validator_addr2: chain_config.get_wallets_config()?.blob_operator.address,
        da_mode,
        dangerous_no_cross_check: false,
    };

    let chain_info = fetch_chain_info(&upgrade_info, &args).await?;

    let admin_calls_finalize = get_admin_call_builder(&upgrade_info, &chain_info, args);

    admin_calls_finalize.display();

    let admin_calldata = admin_calls_finalize.compile_full_calldata();

    call_chain_admin(l1_url, chain_config, admin_calldata).await?;

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
                chain_config.chain_id.as_u64(),
                args.l2_wrapped_base_token_addr
                    .context("l2_wrapped_base_token_addr")?,
            ),
        )
        .unwrap();

    let mut forge = Forge::new(&ecosystem_config.path_to_l1_foundry())
        .script(
            &ACCEPT_GOVERNANCE_SCRIPT_PARAMS.script(),
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
