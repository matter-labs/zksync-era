use std::{num::NonZeroUsize, str::FromStr, sync::Arc};

use anyhow::Context;
use clap::{Parser, ValueEnum};
use ethers::{
    abi::{encode, parse_abi, Token},
    contract::{abigen, BaseContract},
    providers::{Http, Middleware, Provider},
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
    namespaces::{UnstableNamespaceClient, ZksNamespaceClient},
};

/// To support both functionality of assignment inside local tests
/// and to print out the changes to the user the following function is used.
#[macro_export]
macro_rules! assign_or_print {
    ($statement:expr, $value:expr, $should_assign:expr) => {
        if $should_assign {
            $statement = $value;
        } else {
            println!("{} = {:#?}", stringify!($statement), $value);
        }
    };
}

#[macro_export]
macro_rules! amend_config_pre_upgrade {
    () => {
        assign_or_print!()
    };
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct GatewayUpgradeInfo {
    // Information about pre-upgrade contracts.
    l1_chain_id: u32,
    pub(crate) bridgehub_addr: Address,
    old_validator_timelock: Address,
    pub(crate) l1_legacy_shared_bridge: Address,

    // Information about the post-upgrade contracts.
    ctm_deployment_tracker_proxy_addr: Address,
    pub(crate) native_token_vault_addr: Address,
    l1_bytecodes_supplier_addr: Address,
    rollup_l1_da_validator_addr: Address,
    no_da_validium_l1_validator_addr: Address,
    expected_rollup_l2_da_validator: Address,
    expected_validium_l2_da_validator: Address,
    new_validator_timelock: Address,

    l1_wrapped_base_token_store: Address,
    chain_upgrade_diamond_cut: Bytes,

    new_protocol_version: u64,
    old_protocol_version: u64,
}

#[derive(Debug, Default)]
pub struct FetchedChainInfo {
    l2_legacy_shared_bridge_addr: Address,
    hyperchain_addr: Address,
    base_token_addr: Address,
    chain_admin_addr: Address,
}

// Bridgehub ABI
abigen!(
    BridgehubAbi,
    r"[
    function getHyperchain(uint256)(address)
]"
);

// L1SharedBridgeLegacyStore ABI
abigen!(
    L1SharedBridgeLegacyAbi,
    r"[
    function l2BridgeAddress(uint256 _chainId)(address)
]"
);

// L2WrappedBaseTokenStore ABI
abigen!(
    L2WrappedBaseTokenStoreAbi,
    r"[
    function l2WBaseTokenAddress(uint256 _chainId)(address)
]"
);

// ZKChain ABI
abigen!(
    ZKChainAbi,
    r"[
    function getPubdataPricingMode()(uint256)
    function getBaseToken()(address)
    function getAdmin()(address)
    function getTotalBatchesCommitted() external view returns (uint256)
    function getTotalBatchesVerified() external view returns (uint256)
]"
);

// ZKChain ABI
abigen!(
    ValidatorTimelockAbi,
    r"[
    function validators(uint256 _chainId, address _validator)(bool)
]"
);

async fn verify_next_batch_new_version(
    batch_number: u32,
    main_node_client: &DynClient<L2>,
) -> anyhow::Result<()> {
    let (_, right_bound) = main_node_client
        .get_l2_block_range(L1BatchNumber(batch_number))
        .await?
        .context("Range must be present for a batch")?;

    let next_l2_block = right_bound + 1;

    let block_details = main_node_client
        .get_block_details(L2BlockNumber(next_l2_block.as_u32()))
        .await?
        .with_context(|| format!("No L2 block is present after the batch {}", batch_number))?;

    let protocol_version = block_details.protocol_version.with_context(|| {
        format!(
            "Protocol version not present for block {}",
            next_l2_block.as_u64()
        )
    })?;
    anyhow::ensure!(
        protocol_version >= ProtocolVersionId::gateway_upgrade(),
        "THe block does not yet contain the gateway upgrade"
    );

    Ok(())
}

pub async fn check_chain_readiness(
    l1_rpc_url: String,
    l2_rpc_url: String,
    l2_chain_id: u64,
) -> anyhow::Result<()> {
    let l1_provider = match Provider::<Http>::try_from(&l1_rpc_url) {
        Ok(provider) => provider,
        Err(err) => {
            anyhow::bail!("Connection error: {:#?}", err);
        }
    };
    let l1_client = Arc::new(l1_provider);

    let l2_client = Client::http(SensitiveUrl::from_str(&l2_rpc_url).unwrap())
        .context("failed creating JSON-RPC client for main node")?
        .for_network(L2ChainId::new(l2_chain_id).unwrap().into())
        .with_allowed_requests_per_second(NonZeroUsize::new(100_usize).unwrap())
        .build();
    let l2_client = Box::new(l2_client) as Box<DynClient<L2>>;

    let inflight_txs_count = match l2_client.get_unconfirmed_txs_count().await {
        Ok(x) => x,
        Err(e) => {
            anyhow::bail!("Failed to call `unstable_unconfirmedTxsCount`. Reason: `{}`.\nEnsure that `unstable` namespace is enabled on your server and it runs the latest version", e)
        }
    };

    let diamond_proxy_addr = l2_client.get_main_contract().await?;

    if inflight_txs_count != 0 {
        anyhow::bail!("Chain not ready since there are inflight txs!");
    }

    let zkchain = ZKChainAbi::new(diamond_proxy_addr, l1_client.clone());
    let batches_committed = zkchain.get_total_batches_committed().await?.as_u32();
    let batches_verified = zkchain.get_total_batches_verified().await?.as_u32();

    verify_next_batch_new_version(batches_committed, l2_client.as_ref()).await?;
    verify_next_batch_new_version(batches_verified, l2_client.as_ref()).await?;

    Ok(())
}

async fn verify_correct_l2_wrapped_base_token(
    l2_rpc_url: String,
    addr: Address,
) -> anyhow::Result<()> {
    // Connect to the L1 Ethereum network
    let l2_provider = match Provider::<Http>::try_from(&l2_rpc_url) {
        Ok(provider) => provider,
        Err(err) => {
            anyhow::bail!("Connection error: {:#?}", err);
        }
    };

    let code = l2_provider.get_code(addr, None).await?;

    if code.len() == 0 {
        anyhow::bail!("L2 wrapped base token code can not be empty");
    }

    // TODO(EVM-939): also verify that the code is correct.

    Ok(())
}

pub async fn fetch_chain_info(
    upgrade_info: &GatewayUpgradeInfo,
    args: &GatewayUpgradeArgsInner,
) -> anyhow::Result<FetchedChainInfo> {
    // Connect to the L1 Ethereum network
    let provider = match Provider::<Http>::try_from(&args.l1_rpc_url) {
        Ok(provider) => provider,
        Err(err) => {
            anyhow::bail!("Connection error: {:#?}", err);
        }
    };

    let client = Arc::new(provider);
    let chain_id = U256::from(args.chain_id);

    let bridgehub = BridgehubAbi::new(upgrade_info.bridgehub_addr, client.clone());
    let hyperchain_addr = bridgehub.get_hyperchain(chain_id).await?;
    if hyperchain_addr == Address::zero() {
        anyhow::bail!("Chain not present in bridgehub");
    }
    let l1_legacy_bridge =
        L1SharedBridgeLegacyAbi::new(upgrade_info.l1_legacy_shared_bridge, client.clone());

    let l2_legacy_shared_bridge_addr = l1_legacy_bridge.l_2_bridge_address(chain_id).await?;
    // Creation of the shared bridge is one of the steps for chain creation,
    // so it is very weird that a chain does not have it, so we fail here.
    anyhow::ensure!(
        l2_legacy_shared_bridge_addr != Address::zero(),
        "Chain not registered inside the L1 shared bridge!"
    );

    let l2_wrapped_base_token_store =
        L2WrappedBaseTokenStoreAbi::new(upgrade_info.l1_wrapped_base_token_store, client.clone());

    let l2_predeployed_wrapped_base_token = l2_wrapped_base_token_store
        .l_2w_base_token_address(chain_id)
        .await?;

    // Even in case the user does not want the script to fail due to this issue,
    // we still display it just in case.
    if l2_predeployed_wrapped_base_token == Address::zero() && args.dangerous_no_cross_check {
        println!("\n\nWARNING: the chain does not contain wrapped base token. It is dangerous since the security of it depends on the ecosystem admin\n\n");
    }

    let zkchain = ZKChainAbi::new(hyperchain_addr, client.clone());

    let chain_admin_addr = zkchain.get_admin().await?;
    let base_token_addr = zkchain.get_base_token().await?;

    if !args.dangerous_no_cross_check {
        // Firstly, check that the validators are present in the current timelock
        let old_timelock =
            ValidatorTimelockAbi::new(upgrade_info.old_validator_timelock, client.clone());

        if !old_timelock
            .validators(chain_id, args.validator_addr1)
            .await?
        {
            anyhow::bail!(
                "{} not validator",
                hex_address_display(args.validator_addr1)
            );
        }
        if !old_timelock
            .validators(chain_id, args.validator_addr2)
            .await?
        {
            anyhow::bail!(
                "{} not validator",
                hex_address_display(args.validator_addr2)
            );
        }

        if l2_predeployed_wrapped_base_token == Address::zero() {
            anyhow::bail!("the chain does not contain wrapped base token. It is dangerous since the security of it depends on the ecosystem admin");
        }

        verify_correct_l2_wrapped_base_token(
            args.l2_rpc_url.clone(),
            l2_predeployed_wrapped_base_token,
        )
        .await?;

        // Secondly, we check that the DA layer corresponds to the current pubdata pricing mode.

        // On L1 it is an enum with 0 meaaning a rollup and 1 meaning a validium.
        // In the old version, it denoted how the pubdata will be checked. We use it to cross-check the
        // user's input
        let pricing_mode = zkchain.get_pubdata_pricing_mode().await?;
        let pricing_mode_rollup = pricing_mode == U256::zero();

        if args.da_mode.is_rollup() != pricing_mode_rollup {
            anyhow::bail!("DA mode in consistent with the current system");
        }
    }

    Ok(FetchedChainInfo {
        l2_legacy_shared_bridge_addr,
        hyperchain_addr,
        base_token_addr,
        chain_admin_addr,
    })
}

impl ZkStackConfig for GatewayUpgradeInfo {}

pub fn encode_ntv_asset_id(l1_chain_id: U256, addr: Address) -> H256 {
    let encoded_data = encode(&[
        ethers::abi::Token::Uint(l1_chain_id),
        ethers::abi::Token::Address(L2_NATIVE_TOKEN_VAULT_ADDRESS),
        ethers::abi::Token::Address(addr),
    ]);

    H256(keccak256(&encoded_data))
}

impl GatewayUpgradeInfo {
    pub fn from_gateway_ecosystem_upgrade(
        bridgehub_addr: Address,
        gateway_ecosystem_upgrade: GatewayEcosystemUpgradeOutput,
    ) -> Self {
        Self {
            l1_chain_id: gateway_ecosystem_upgrade.l1_chain_id,
            bridgehub_addr,
            old_validator_timelock: gateway_ecosystem_upgrade
                .contracts_config
                .old_validator_timelock,
            l1_legacy_shared_bridge: gateway_ecosystem_upgrade
                .contracts_config
                .l1_legacy_shared_bridge,
            ctm_deployment_tracker_proxy_addr: gateway_ecosystem_upgrade
                .deployed_addresses
                .bridgehub
                .ctm_deployment_tracker_proxy_addr,
            native_token_vault_addr: gateway_ecosystem_upgrade
                .deployed_addresses
                .native_token_vault_addr,
            l1_bytecodes_supplier_addr: gateway_ecosystem_upgrade
                .deployed_addresses
                .l1_bytecodes_supplier_addr,
            rollup_l1_da_validator_addr: gateway_ecosystem_upgrade
                .deployed_addresses
                .rollup_l1_da_validator_addr,
            no_da_validium_l1_validator_addr: gateway_ecosystem_upgrade
                .deployed_addresses
                .validium_l1_da_validator_addr,
            expected_rollup_l2_da_validator: gateway_ecosystem_upgrade
                .contracts_config
                .expected_rollup_l2_da_validator,
            expected_validium_l2_da_validator: gateway_ecosystem_upgrade
                .contracts_config
                .expected_validium_l2_da_validator,
            new_validator_timelock: gateway_ecosystem_upgrade
                .deployed_addresses
                .validator_timelock_addr,
            // Note that on the contract side of things this contract is called `L2WrappedBaseTokenStore`,
            // while on the server side for consistency with the conventions, where the prefix denotes
            // the location of the contracts we call it `l1_wrapped_base_token_store`
            l1_wrapped_base_token_store: gateway_ecosystem_upgrade
                .deployed_addresses
                .l2_wrapped_base_token_store_addr,
            chain_upgrade_diamond_cut: gateway_ecosystem_upgrade.chain_upgrade_diamond_cut,
            new_protocol_version: gateway_ecosystem_upgrade
                .contracts_config
                .new_protocol_version,
            old_protocol_version: gateway_ecosystem_upgrade
                .contracts_config
                .old_protocol_version,
        }
    }

    fn get_l1_da_validator(&self, da_mode: DAMode) -> Address {
        if da_mode.is_rollup() {
            self.rollup_l1_da_validator_addr
        } else {
            self.no_da_validium_l1_validator_addr
        }
    }

    fn get_l2_da_validator(&self, da_mode: DAMode) -> Address {
        if da_mode.is_rollup() {
            self.expected_rollup_l2_da_validator
        } else {
            self.expected_validium_l2_da_validator
        }
    }

    pub fn update_contracts_config(
        &self,
        contracts_config: &mut ContractsConfig,
        chain_info: &FetchedChainInfo,
        da_mode: DAMode,
        assign: bool,
    ) {
        assign_or_print!(
            contracts_config.l2.legacy_shared_bridge_addr,
            Some(chain_info.l2_legacy_shared_bridge_addr),
            assign
        );

        let base_token_id =
            encode_ntv_asset_id(U256::from(self.l1_chain_id), chain_info.base_token_addr);
        assign_or_print!(
            contracts_config.l1.base_token_asset_id,
            Some(base_token_id),
            assign
        );

        assign_or_print!(
            contracts_config
                .ecosystem_contracts
                .l1_wrapped_base_token_store,
            Some(self.l1_wrapped_base_token_store),
            assign
        );

        assign_or_print!(
            contracts_config
                .ecosystem_contracts
                .stm_deployment_tracker_proxy_addr,
            Some(self.ctm_deployment_tracker_proxy_addr),
            assign
        );
        assign_or_print!(
            contracts_config.ecosystem_contracts.native_token_vault_addr,
            Some(self.native_token_vault_addr),
            assign
        );
        assign_or_print!(
            contracts_config
                .ecosystem_contracts
                .l1_bytecodes_supplier_addr,
            Some(self.l1_bytecodes_supplier_addr),
            assign
        );
        assign_or_print!(
            contracts_config.l1.rollup_l1_da_validator_addr,
            Some(self.rollup_l1_da_validator_addr),
            assign
        );
        assign_or_print!(
            contracts_config.l1.no_da_validium_l1_validator_addr,
            Some(self.no_da_validium_l1_validator_addr),
            assign
        );

        assign_or_print!(
            contracts_config.l2.da_validator_addr,
            Some(self.get_l2_da_validator(da_mode)),
            assign
        );

        assign_or_print!(
            contracts_config.l2.l2_native_token_vault_proxy_addr,
            Some(L2_NATIVE_TOKEN_VAULT_ADDRESS),
            assign
        );
    }

    // Updates to the config that should be done somewhere after the upgrade is fully over.
    // They do not have to updated for the system to work smoothly during the upgrade, but after
    // "stage 2" they are desirable to be updated for consistency
    pub fn _post_upgrade_update_contracts_config(
        &self,
        _config: &mut ContractsConfig,
        _assign: bool,
    ) {
        todo!()
    }
}

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, ValueEnum, EnumIter, strum::Display, PartialEq, Eq,
)]
pub(crate) enum DAMode {
    Validium,
    TemporaryRollup,
    PermanentRollup,
}

impl DAMode {
    fn is_rollup(&self) -> bool {
        matches!(self, Self::TemporaryRollup | Self::PermanentRollup)
    }
}

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
            zkchain_abi: hyperchain_contract(),
            chain_admin_abi: chain_admin_contract(),
        }
    }

    pub fn append_validator(
        &mut self,
        chain_id: u64,
        validator_timelock_addr: Address,
        validator_addr: Address,
    ) {
        let data = self
            .validator_timelock_abi
            .encode("addValidator", (U256::from(chain_id), validator_addr))
            .unwrap();
        let description = format!(
            "Adding validator 0x{}",
            hex::encode(validator_timelock_addr.0)
        );

        let call = AdminCall {
            description,
            data: data.to_vec(),
            target: validator_timelock_addr,
            value: U256::zero(),
        };

        self.calls.push(call);
    }

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

    pub fn append_set_da_validator_pair(
        &mut self,
        hyperchain_addr: Address,
        l1_da_validator: Address,
        l2_da_validator: Address,
    ) {
        let data = self
            .zkchain_abi
            .function("setDAValidatorPair")
            .unwrap()
            .encode_input(&[
                Token::Address(l1_da_validator),
                Token::Address(l2_da_validator),
            ])
            .unwrap();
        let description = "Setting DA validator pair".to_string();

        let call = AdminCall {
            description,
            data: data.to_vec(),
            target: hyperchain_addr,
            value: U256::zero(),
        };

        self.calls.push(call);
    }

    pub fn append_make_permanent_rollup(&mut self, hyperchain_addr: Address) {
        let data = self
            .zkchain_abi
            .function("makePermanentRollup")
            .unwrap()
            .encode_input(&[])
            .unwrap();
        let description = "Make permanent rollup:".to_string();

        let call = AdminCall {
            description,
            data: data.to_vec(),
            target: hyperchain_addr,
            value: U256::zero(),
        };

        self.calls.push(call);
    }

    pub fn display(&self) {
        // Serialize with pretty printing
        let serialized = serde_json::to_string_pretty(&self.calls).unwrap();

        // Output the serialized JSON
        println!("{}", serialized);
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

fn chain_admin_abi() -> BaseContract {
    BaseContract::from(
        parse_abi(&[
            "function setUpgradeTimestamp(uint256 _protocolVersion, uint256 _upgradeTimestamp) external",
        ])
        .unwrap(),
    )
}

pub fn set_upgrade_timestamp_calldata(packed_protocol_version: u64, timestamp: u64) -> Vec<u8> {
    let chain_admin = chain_admin_abi();

    chain_admin
        .encode("setUpgradeTimestamp", (packed_protocol_version, timestamp))
        .unwrap()
        .to_vec()
}

#[derive(Parser, Debug, Clone)]
pub struct GatewayUpgradeCalldataArgs {
    upgrade_description_path: String,
    chain_id: u64,
    l1_rpc_url: String,
    l2_rpc_url: String,
    validator_addr1: Address,
    validator_addr2: Address,
    server_upgrade_timestamp: u64,
    da_mode: DAMode,
    #[clap(long, default_missing_value = "false")]
    dangerous_no_cross_check: Option<bool>,
    #[clap(long, default_missing_value = "false")]
    force_display_finalization_params: Option<bool>,
}

pub struct GatewayUpgradeArgsInner {
    pub chain_id: u64,
    pub l1_rpc_url: String,
    pub l2_rpc_url: String,
    pub validator_addr1: Address,
    pub validator_addr2: Address,
    pub da_mode: DAMode,
    pub dangerous_no_cross_check: bool,
}

impl From<GatewayUpgradeCalldataArgs> for GatewayUpgradeArgsInner {
    fn from(value: GatewayUpgradeCalldataArgs) -> Self {
        Self {
            chain_id: value.chain_id,
            l1_rpc_url: value.l1_rpc_url,
            l2_rpc_url: value.l2_rpc_url,
            validator_addr1: value.validator_addr1,
            validator_addr2: value.validator_addr2,
            da_mode: value.da_mode,
            dangerous_no_cross_check: value.dangerous_no_cross_check.unwrap_or_default(),
        }
    }
}

pub fn get_admin_call_builder(
    upgrade_info: &GatewayUpgradeInfo,
    chain_info: &FetchedChainInfo,
    args: GatewayUpgradeArgsInner,
) -> AdminCallBuilder {
    let mut admin_calls_finalize = AdminCallBuilder::new();

    admin_calls_finalize.append_validator(
        args.chain_id,
        upgrade_info.new_validator_timelock,
        args.validator_addr1,
    );
    admin_calls_finalize.append_validator(
        args.chain_id,
        upgrade_info.new_validator_timelock,
        args.validator_addr2,
    );

    admin_calls_finalize.append_execute_upgrade(
        chain_info.hyperchain_addr,
        upgrade_info.old_protocol_version,
        upgrade_info.chain_upgrade_diamond_cut.clone(),
    );

    admin_calls_finalize.append_set_da_validator_pair(
        chain_info.hyperchain_addr,
        upgrade_info.get_l1_da_validator(args.da_mode),
        upgrade_info.get_l2_da_validator(args.da_mode),
    );

    if args.da_mode == DAMode::PermanentRollup {
        admin_calls_finalize.append_make_permanent_rollup(chain_info.hyperchain_addr);
    }

    admin_calls_finalize
}

pub(crate) async fn run(shell: &Shell, args: GatewayUpgradeCalldataArgs) -> anyhow::Result<()> {
    // 0. Read the GatewayUpgradeInfo

    let upgrade_info = GatewayUpgradeInfo::read(shell, &args.upgrade_description_path)?;

    // 1. Update all the configs

    let chain_info = fetch_chain_info(&upgrade_info, &args.clone().into()).await?;

    upgrade_info.update_contracts_config(&mut Default::default(), &chain_info, args.da_mode, false);

    // 2. Generate calldata

    let schedule_calldata = set_upgrade_timestamp_calldata(
        upgrade_info.new_protocol_version,
        args.server_upgrade_timestamp,
    );

    let set_timestamp_call = AdminCall {
        description: "Calldata to schedule upgrade".to_string(),
        data: schedule_calldata,
        target: chain_info.chain_admin_addr,
        value: U256::zero(),
    };
    println!("{}", serde_json::to_string_pretty(&set_timestamp_call)?);
    println!("---------------------------");

    if !args.force_display_finalization_params.unwrap_or_default() {
        let chain_readiness = check_chain_readiness(
            args.l1_rpc_url.clone(),
            args.l2_rpc_url.clone(),
            args.chain_id,
        )
        .await;

        if let Err(err) = chain_readiness {
            println!(
                "Chain is not ready to finalize the upgrade due to the reason:\n{:#?}",
                err
            );
            println!("Once the chain is ready, you can re-run this command to obtain the calls to finalize the upgrade");
            println!("If you want to display finalization params anyway, pass `--force-display-finalization-params=true`.");
            return Ok(());
        };
    }

    let admin_calls_finalize = get_admin_call_builder(&upgrade_info, &chain_info, args.into());

    admin_calls_finalize.display();

    let chain_admin_calldata = admin_calls_finalize.compile_full_calldata();

    println!(
        "Full calldata to call `ChainAdmin` with : {}",
        hex::encode(&chain_admin_calldata)
    );

    Ok(())
}
