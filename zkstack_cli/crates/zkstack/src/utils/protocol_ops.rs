use std::path::PathBuf;

use ethers::types::{Address, H256};
use serde::Deserialize;
use xshell::Shell;
use zkstack_cli_common::contracts::encode_ntv_asset_id;
use zkstack_cli_config::{
    BridgeContractsDefinition, BridgesContracts, ChainConfig, ChainTransitionManagerContracts,
    ContractsConfig, CoreContractsConfig, CoreEcosystemContracts, EcosystemConfig,
    EcosystemContracts, L1Contracts, L1CoreContracts, L2Contracts,
};
use zkstack_cli_types::VMOption;

/// Arguments for invoking `protocol_ops ecosystem init`.
pub struct EcosystemInitProtocolOpsArgs {
    /// Deployer private key (sender)
    pub private_key: H256,
    /// Governor address (owner of deployed contracts)
    pub owner: Address,
    /// Governor private key (for accepting ownership when owner != sender)
    pub owner_private_key: Option<H256>,
    pub l1_rpc_url: String,
    pub era_chain_id: u64,
    pub vm_type: VMOption,
    pub with_testnet_verifier: bool,
    pub with_legacy_bridge: bool,
}

/// Runner that invokes protocol_ops CLI via `cargo run`.
pub struct ProtocolOpsRunner {
    manifest_path: PathBuf,
}

/// Arguments for invoking `protocol_ops chain init`.
pub struct ChainInitProtocolOpsArgs {
    pub private_key: H256,
    /// Chain governor private key (for accepting ownership when owner != sender)
    pub owner_private_key: Option<H256>,
    pub l1_rpc_url: String,
    pub ctm_proxy: Address,
    pub l1_da_validator: Address,
    pub chain_id: u64,
    pub base_token_addr: Address,
    pub base_token_price_ratio: String,
    pub da_mode: String,
    pub vm_type: VMOption,
    pub owner: Address,
    pub commit_operator: Address,
    pub prove_operator: Address,
    pub execute_operator: Option<Address>,
    pub token_multiplier_setter: Option<Address>,
    pub governance_addr: Address,
    pub with_legacy_bridge: bool,
    pub pause_deposits: bool,
    pub evm_emulator: bool,
    pub deploy_paymaster: bool,
    pub make_permanent_rollup: bool,
    pub skip_priority_txs: bool,
    pub create2_factory_addr: Option<Address>,
    pub create2_factory_salt: Option<H256>,
}

impl ProtocolOpsRunner {
    pub fn new(ecosystem_config: &EcosystemConfig) -> Self {
        let manifest_path = ecosystem_config
            .link_to_code()
            .join("contracts/protocol-ops/Cargo.toml");
        Self { manifest_path }
    }

    pub fn ecosystem_init(
        &self,
        shell: &Shell,
        args: &EcosystemInitProtocolOpsArgs,
    ) -> anyhow::Result<ProtocolOpsEcosystemOutput> {
        let out_file = tempfile::NamedTempFile::new()?;
        let out_path = out_file.path().to_str().unwrap().to_string();

        let manifest_path = self.manifest_path.display().to_string();
        let pk = format!("{:#x}", args.private_key);
        let l1_rpc_url = &args.l1_rpc_url;
        let era_chain_id = args.era_chain_id.to_string();
        let vm_type = match args.vm_type {
            VMOption::EraVM => "eravm",
            VMOption::ZKSyncOsVM => "zksyncos",
        };

        let owner = format!("{:#x}", args.owner);
        let with_testnet_verifier =
            format!("--with-testnet-verifier={}", args.with_testnet_verifier);
        let with_legacy_bridge = format!("--with-legacy-bridge={}", args.with_legacy_bridge);

        let mut extra_args: Vec<String> = Vec::new();
        if let Some(owner_pk) = args.owner_private_key {
            extra_args.push(format!("--owner-private-key={:#x}", owner_pk));
        }

        xshell::cmd!(
            shell,
            "cargo run --manifest-path {manifest_path} --bin protocol_ops --release --
             ecosystem init
             --pk {pk}
             --owner {owner}
             --l1-rpc-url {l1_rpc_url}
             --era-chain-id {era_chain_id}
             --vm-type {vm_type}
             {with_testnet_verifier}
             {with_legacy_bridge}
             {extra_args...}
             --out {out_path}"
        )
        .run()?;

        let json_str = std::fs::read_to_string(&out_path)?;
        let output: ProtocolOpsJsonOutput = serde_json::from_str(&json_str)?;
        Ok(output.output)
    }

    pub fn chain_init(
        &self,
        shell: &Shell,
        args: &ChainInitProtocolOpsArgs,
    ) -> anyhow::Result<ProtocolOpsChainOutput> {
        let out_file = tempfile::NamedTempFile::new()?;
        let out_path = out_file.path().to_str().unwrap().to_string();

        let manifest_path = self.manifest_path.display().to_string();
        let pk = format!("{:#x}", args.private_key);
        let l1_rpc_url = &args.l1_rpc_url;
        let ctm_proxy = format!("{:#x}", args.ctm_proxy);
        let l1_da_validator = format!("{:#x}", args.l1_da_validator);
        let chain_id = args.chain_id.to_string();
        let base_token_addr = format!("{:#x}", args.base_token_addr);
        let base_token_price_ratio = &args.base_token_price_ratio;
        let da_mode = &args.da_mode;
        let vm_type = match args.vm_type {
            VMOption::EraVM => "eravm",
            VMOption::ZKSyncOsVM => "zksyncos",
        };
        let owner = format!("{:#x}", args.owner);
        let commit_operator = format!("{:#x}", args.commit_operator);
        let prove_operator = format!("{:#x}", args.prove_operator);
        let governance_addr = format!("{:#x}", args.governance_addr);
        let with_legacy_bridge = format!("--with-legacy-bridge={}", args.with_legacy_bridge);
        let pause_deposits = format!("--pause-deposits={}", args.pause_deposits);
        let evm_emulator = format!("--evm-emulator={}", args.evm_emulator);
        let deploy_paymaster_flag = format!("--deploy-paymaster={}", args.deploy_paymaster);
        let make_permanent_rollup =
            format!("--make-permanent-rollup={}", args.make_permanent_rollup);
        let skip_priority_txs = format!("--skip-priority-txs={}", args.skip_priority_txs);

        let mut extra_args: Vec<String> = Vec::new();

        if let Some(owner_pk) = args.owner_private_key {
            extra_args.push(format!("--owner-private-key={:#x}", owner_pk));
        }
        if let Some(exec_op) = args.execute_operator {
            extra_args.push(format!("--execute-operator={:#x}", exec_op));
        }
        if let Some(tms) = args.token_multiplier_setter {
            extra_args.push(format!("--token-multiplier-setter={:#x}", tms));
        }
        if let Some(c2f) = args.create2_factory_addr {
            extra_args.push(format!("--create2-factory-addr={:#x}", c2f));
        }
        if let Some(c2s) = args.create2_factory_salt {
            extra_args.push(format!("--create2-factory-salt={:#x}", c2s));
        }
        xshell::cmd!(
            shell,
            "cargo run --manifest-path {manifest_path} --bin protocol_ops --release --
             chain init
             --pk {pk}
             --l1-rpc-url {l1_rpc_url}
             --ctm-proxy {ctm_proxy}
             --l1-da-validator {l1_da_validator}
             --chain-id {chain_id}
             --base-token-addr {base_token_addr}
             --base-token-price-ratio {base_token_price_ratio}
             --da-mode {da_mode}
             --vm-type {vm_type}
             --owner {owner}
             --commit-operator {commit_operator}
             --prove-operator {prove_operator}
             --governance-addr {governance_addr}
             {with_legacy_bridge}
             {pause_deposits}
             {evm_emulator}
             {deploy_paymaster_flag}
             {make_permanent_rollup}
             {skip_priority_txs}
             {extra_args...}
             --out {out_path}"
        )
        .run()?;

        let json_str = std::fs::read_to_string(&out_path)?;
        let output: ProtocolOpsChainJsonOutput = serde_json::from_str(&json_str)?;
        Ok(output.output)
    }
}

/// Top-level JSON structure from protocol_ops `--out` file.
#[derive(Debug, Deserialize)]
struct ProtocolOpsJsonOutput {
    pub output: ProtocolOpsEcosystemOutput,
}

/// The `"output"` object from protocol_ops ecosystem init JSON.
#[derive(Debug, Deserialize)]
pub struct ProtocolOpsEcosystemOutput {
    pub hub: HubOutput,
    pub ctm: CtmOutput,
}

#[derive(Debug, Deserialize)]
pub struct HubOutput {
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
    pub bridgehub_proxy_addr: Address,
    pub message_root_proxy_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    pub stm_deployment_tracker_proxy_addr: Address,
    pub native_token_vault_addr: Address,
    pub chain_asset_handler_proxy_addr: Address,
    pub shared_bridge_proxy_addr: Address,
    pub erc20_bridge_proxy_addr: Address,
    pub l1_nullifier_proxy_addr: Address,
    pub governance_addr: Address,
    pub chain_admin_addr: Address,
    pub access_control_restriction_addr: Address,
}

#[derive(Debug, Deserialize)]
pub struct CtmOutput {
    pub state_transition_proxy_addr: Address,
    pub verifier_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub default_upgrade_addr: Address,
    pub bytecodes_supplier_addr: Address,
    pub validator_timelock_addr: Address,
    pub rollup_l1_da_validator_addr: Address,
    pub no_da_validium_l1_validator_addr: Address,
    pub avail_l1_da_validator_addr: Address,
    pub l1_rollup_da_manager: Address,
    pub blobs_zksync_os_l1_da_validator_addr: Option<Address>,
    pub server_notifier_proxy_addr: Address,
    pub governance_addr: Address,
    pub chain_admin_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    pub multicall3_addr: Address,
    pub diamond_cut_data: String,
    pub force_deployments_data: Option<String>,
}

/// Top-level JSON structure from protocol_ops chain init `--out` file.
#[derive(Debug, Deserialize)]
struct ProtocolOpsChainJsonOutput {
    pub output: ProtocolOpsChainOutput,
}

/// The `"output"` object from protocol_ops chain init JSON.
#[derive(Debug, Deserialize)]
pub struct ProtocolOpsChainOutput {
    pub diamond_proxy_addr: Address,
    #[serde(default)]
    pub governance_addr: Option<Address>,
    pub chain_admin_addr: Address,
    #[serde(default)]
    pub access_control_restriction_addr: Option<Address>,
    #[serde(default)]
    pub chain_proxy_admin_addr: Option<Address>,
    #[serde(default)]
    pub l2_legacy_shared_bridge_addr: Option<Address>,
    #[serde(default)]
    pub l2_contracts: Option<ProtocolOpsL2ContractsOutput>,
    #[serde(default)]
    pub paymaster_addr: Option<Address>,
}

/// L2 contracts output from protocol_ops chain init.
#[derive(Debug, Deserialize)]
pub struct ProtocolOpsL2ContractsOutput {
    pub l2_default_upgrader: Address,
    pub consensus_registry_addr: Address,
    pub multicall3_addr: Address,
    pub timestamp_asserter_addr: Address,
}

impl ProtocolOpsChainOutput {
    /// Build a full `ContractsConfig` from the protocol_ops output + ecosystem contracts.
    pub fn to_contracts_config(
        &self,
        core_contracts: &CoreContractsConfig,
        chain_config: &ChainConfig,
    ) -> ContractsConfig {
        let ctm = core_contracts.ctm(chain_config.vm_option);

        let mut contracts = ContractsConfig {
            create2_factory_addr: core_contracts.create2_factory_addr,
            create2_factory_salt: core_contracts.create2_factory_salt,
            ecosystem_contracts: EcosystemContracts {
                bridgehub_proxy_addr: core_contracts.core_ecosystem_contracts.bridgehub_proxy_addr,
                message_root_proxy_addr: core_contracts
                    .core_ecosystem_contracts
                    .message_root_proxy_addr,
                transparent_proxy_admin_addr: core_contracts
                    .core_ecosystem_contracts
                    .transparent_proxy_admin_addr,
                stm_deployment_tracker_proxy_addr: core_contracts
                    .core_ecosystem_contracts
                    .stm_deployment_tracker_proxy_addr,
                native_token_vault_addr: core_contracts
                    .core_ecosystem_contracts
                    .native_token_vault_addr,
                chain_asset_handler_proxy_addr: core_contracts
                    .core_ecosystem_contracts
                    .chain_asset_handler_proxy_addr,
                ctm: ctm.clone(),
            },
            bridges: core_contracts.bridges.clone(),
            proof_manager_contracts: core_contracts.proof_manager_contracts.clone(),
            l1: L1Contracts {
                default_upgrade_addr: ctm.default_upgrade_addr,
                diamond_proxy_addr: self.diamond_proxy_addr,
                governance_addr: self.governance_addr.unwrap_or(ctm.governance),
                chain_admin_addr: self.chain_admin_addr,
                access_control_restriction_addr: self.access_control_restriction_addr,
                chain_proxy_admin_addr: self.chain_proxy_admin_addr,
                validator_timelock_addr: ctm.validator_timelock_addr,
                base_token_addr: chain_config.base_token.address,
                rollup_l1_da_validator_addr: Some(ctm.rollup_l1_da_validator_addr),
                blobs_zksync_os_l1_da_validator_addr: ctm.blobs_zksync_os_l1_da_validator_addr,
                transaction_filterer_addr: core_contracts.l1.transaction_filterer_addr,
                verifier_addr: ctm.verifier_addr,
                base_token_asset_id: Some(encode_ntv_asset_id(
                    chain_config.l1_network.chain_id().into(),
                    chain_config.base_token.address,
                )),
                multicall3_addr: core_contracts.multicall3_addr,
                avail_l1_da_validator_addr: Some(ctm.avail_l1_da_validator_addr),
                no_da_validium_l1_validator_addr: Some(ctm.no_da_validium_l1_validator_addr),
            },
            l2: L2Contracts {
                legacy_shared_bridge_addr: self.l2_legacy_shared_bridge_addr,
                ..Default::default()
            },
            other: Default::default(),
        };

        // Fill L2 contract addresses if available
        if let Some(l2) = &self.l2_contracts {
            contracts.l2.default_l2_upgrader = l2.l2_default_upgrader;
            contracts.l2.consensus_registry = Some(l2.consensus_registry_addr);
            contracts.l2.multicall3 = Some(l2.multicall3_addr);
            contracts.l2.timestamp_asserter_addr = Some(l2.timestamp_asserter_addr);
        }

        if let Some(paymaster) = self.paymaster_addr {
            contracts.l2.testnet_paymaster_addr = paymaster;
        }

        contracts
    }
}

impl ProtocolOpsEcosystemOutput {
    /// Convert the protocol_ops output into zkstack's `CoreContractsConfig`.
    pub fn to_core_contracts_config(&self, vm_option: VMOption) -> CoreContractsConfig {
        let hub = &self.hub;
        let ctm = &self.ctm;

        let ctm_contracts = ChainTransitionManagerContracts {
            governance: ctm.governance_addr,
            chain_admin: ctm.chain_admin_addr,
            proxy_admin: ctm.transparent_proxy_admin_addr,
            state_transition_proxy_addr: ctm.state_transition_proxy_addr,
            validator_timelock_addr: ctm.validator_timelock_addr,
            diamond_cut_data: ctm.diamond_cut_data.clone(),
            force_deployments_data: ctm.force_deployments_data.clone(),
            l1_bytecodes_supplier_addr: ctm.bytecodes_supplier_addr,
            l1_wrapped_base_token_store: None,
            server_notifier_proxy_addr: ctm.server_notifier_proxy_addr,
            default_upgrade_addr: ctm.default_upgrade_addr,
            genesis_upgrade_addr: ctm.genesis_upgrade_addr,
            verifier_addr: ctm.verifier_addr,
            rollup_l1_da_validator_addr: ctm.rollup_l1_da_validator_addr,
            no_da_validium_l1_validator_addr: ctm.no_da_validium_l1_validator_addr,
            avail_l1_da_validator_addr: ctm.avail_l1_da_validator_addr,
            l1_rollup_da_manager: ctm.l1_rollup_da_manager,
            blobs_zksync_os_l1_da_validator_addr: ctm.blobs_zksync_os_l1_da_validator_addr,
        };

        let (era_ctm, zksync_os_ctm) = match vm_option {
            VMOption::EraVM => (Some(ctm_contracts), None),
            VMOption::ZKSyncOsVM => (None, Some(ctm_contracts)),
        };

        CoreContractsConfig {
            create2_factory_addr: hub.create2_factory_addr,
            create2_factory_salt: hub.create2_factory_salt,
            multicall3_addr: ctm.multicall3_addr,
            core_ecosystem_contracts: CoreEcosystemContracts {
                bridgehub_proxy_addr: hub.bridgehub_proxy_addr,
                message_root_proxy_addr: Some(hub.message_root_proxy_addr),
                transparent_proxy_admin_addr: hub.transparent_proxy_admin_addr,
                stm_deployment_tracker_proxy_addr: Some(hub.stm_deployment_tracker_proxy_addr),
                native_token_vault_addr: Some(hub.native_token_vault_addr),
                chain_asset_handler_proxy_addr: Some(hub.chain_asset_handler_proxy_addr),
            },
            bridges: BridgesContracts {
                erc20: BridgeContractsDefinition {
                    l1_address: hub.erc20_bridge_proxy_addr,
                    l2_address: None,
                },
                shared: BridgeContractsDefinition {
                    l1_address: hub.shared_bridge_proxy_addr,
                    l2_address: None,
                },
                l1_nullifier_addr: Some(hub.l1_nullifier_proxy_addr),
            },
            l1: L1CoreContracts {
                governance_addr: hub.governance_addr,
                chain_admin_addr: hub.chain_admin_addr,
                access_control_restriction_addr: Some(hub.access_control_restriction_addr),
                chain_proxy_admin_addr: None,
                transaction_filterer_addr: None,
            },
            era_ctm,
            zksync_os_ctm,
            proof_manager_contracts: None,
        }
    }
}
