import { Command } from 'commander';
import * as utils from 'utils';
import * as env from './env';
import fs from 'fs';
import { Wallet } from 'ethers';
import path from 'path';

export async function build(): Promise<void> {
    await utils.spawn('yarn l1-contracts build');
    await utils.spawn('yarn l2-contracts build');
}

export async function verifyL1Contracts(): Promise<void> {
    // Spawning a new script is expensive, so if we know that publishing is disabled, it's better to not launch
    // it at all (even though `verify` checks the network as well).
    if (process.env.CHAIN_ETH_NETWORK == 'localhost') {
        console.log('Skip contract verification on localhost');
        return;
    }
    await utils.spawn('yarn l1-contracts verify');
}

export function updateContractsEnv(initEnv: string, deployLog: String, envVars: Array<string>): string {
    let updatedContracts = '';
    for (const envVar of envVars) {
        const pattern = new RegExp(`${envVar}=.*`, 'g');
        const matches = deployLog.match(pattern);
        if (matches !== null) {
            const varContents = matches[0];
            env.modify(envVar, varContents, initEnv, false);
            updatedContracts += `${varContents}\n`;
        }
    }
    env.reload();
    return updatedContracts;
}

export async function initializeValidator(args: any[]): Promise<void> {
    await utils.confirmAction();
    await utils.spawn(`yarn l1-contracts initialize-validator ${args.join(' ')} | tee initializeValidator.log`);
}

export async function initializeGovernance(): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.GOVERNANCE_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];

    await utils.spawn(`yarn l1-contracts initialize-governance ${args.join(' ')} | tee initializeGovernance.log`);
}

export async function deployL2(args: any[] = [], includePaymaster?: boolean): Promise<void> {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    if (!isLocalSetup) {
        await utils.spawn(`yarn l2-contracts build`);
    }

    await utils.spawn(`yarn l2-contracts deploy-shared-bridge-on-l2 ${args.join(' ')} | tee deployL2.log`);

    if (includePaymaster) {
        await utils.spawn(`yarn l2-contracts deploy-testnet-paymaster ${args.join(' ')} | tee -a deployL2.log`);
    }

    await utils.spawn(`yarn l2-contracts deploy-force-deploy-upgrader ${args.join(' ')} | tee -a deployL2.log`);

    let l2DeployLog = fs.readFileSync('deployL2.log').toString();
    const l2DeploymentEnvVars = [
        'CONTRACTS_L2_SHARED_BRIDGE_ADDR',
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
        'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR'
    ];
    updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, l2DeployLog, l2DeploymentEnvVars);
}

// for testnet and development purposes it is ok to deploy contracts form L1.
export async function deployL2ThroughL1({
    includePaymaster = true,
    localLegacyBridgeTesting
}: {
    includePaymaster: boolean;
    localLegacyBridgeTesting?: boolean;
}): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    if (!isLocalSetup) {
        await utils.spawn(`yarn l2-contracts build`);
    }

    await utils.spawn(
        `yarn l2-contracts deploy-shared-bridge-on-l2-through-l1 ${args.join(' ')} ${
            localLegacyBridgeTesting ? '--local-legacy-bridge-testing' : ''
        } | tee deployL2.log`
    );

    if (includePaymaster) {
        await utils.spawn(
            `yarn l2-contracts deploy-testnet-paymaster-through-l1 ${args.join(' ')} | tee -a deployL2.log`
        );
    }

    await utils.spawn(
        `yarn l2-contracts deploy-force-deploy-upgrader-through-l1 ${args.join(' ')} | tee -a deployL2.log`
    );

    let l2DeployLog = fs.readFileSync('deployL2.log').toString();
    const l2DeploymentEnvVars = [
        'CONTRACTS_L2_SHARED_BRIDGE_ADDR',
        'CONTRACTS_L2_ERC20_BRIDGE_ADDR',
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
        'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR'
    ];
    updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, l2DeployLog, l2DeploymentEnvVars);
    // erc20 bridge is now deployed as shared bridge, but we still need the config var:
    updateContractsEnv(
        `etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`,
        `CONTRACTS_L2_ERC20_BRIDGE_ADDR=${process.env.CONTRACTS_L2_SHARED_BRIDGE_ADDR}`,
        l2DeploymentEnvVars
    );
}

async function _deployL1(onlyVerifier: boolean): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : '', onlyVerifier ? '--only-verifier' : ''];

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.

    await utils.spawn(`yarn l1-contracts deploy-no-build ${args.join(' ')} | tee deployL1.log`);
    const deployLog = fs.readFileSync('deployL1.log').toString();
    const l1EnvVars = [
        'CONTRACTS_CREATE2_FACTORY_ADDR',

        'CONTRACTS_BRIDGEHUB_PROXY_ADDR',
        'CONTRACTS_BRIDGEHUB_IMPL_ADDR',

        'CONTRACTS_STATE_TRANSITION_PROXY_ADDR',
        'CONTRACTS_STATE_TRANSITION_IMPL_ADDR',

        'CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR',
        'CONTRACTS_DIAMOND_INIT_ADDR',
        'CONTRACTS_DEFAULT_UPGRADE_ADDR',
        'CONTRACTS_GENESIS_UPGRADE_ADDR',
        'CONTRACTS_GOVERNANCE_ADDR',
        'CONTRACTS_CHAIN_ADMIN_ADDR',
        'CONTRACTS_ADMIN_FACET_ADDR',
        'CONTRACTS_EXECUTOR_FACET_ADDR',
        'CONTRACTS_GETTERS_FACET_ADDR',
        'CONTRACTS_MAILBOX_FACET_ADDR',

        'CONTRACTS_VERIFIER_ADDR',
        'CONTRACTS_VALIDATOR_TIMELOCK_ADDR',

        'CONTRACTS_GENESIS_TX_HASH',
        'CONTRACTS_TRANSPARENT_PROXY_ADMIN_ADDR',
        'CONTRACTS_L1_SHARED_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_SHARED_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_ERC20_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ERC20_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_WETH_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_WETH_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ALLOW_LIST_ADDR',
        'CONTRACTS_L1_MULTICALL3_ADDR',

        /// temporary:
        'CONTRACTS_HYPERCHAIN_UPGRADE_ADDR'
    ];

    console.log('Writing to', `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`);
    const updatedContracts = updateContractsEnv(
        `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`,
        deployLog,
        l1EnvVars
    );

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('deployed_contracts.log', updatedContracts);
}

export enum DeploymentMode {
    Rollup = 0,
    Validium = 1
}

export async function redeployL1(verifierOnly: boolean) {
    await _deployL1(verifierOnly);
    await verifyL1Contracts();
}

export async function wethBridgeFinish(args: any[] = []): Promise<void> {
    await utils.confirmAction();
    await utils.spawn(`yarn l1-contracts weth-finish-deployment-on-chain ${args.join(' ')} | tee -a deployL2.log`);
}

export async function erc20BridgeFinish(args: any[] = []): Promise<void> {
    await utils.confirmAction();
    await utils.spawn(`yarn l1-contracts erc20-finish-deployment-on-chain ${args.join(' ')} | tee -a deployL2.log`);
}

export async function registerHyperchain({
    baseTokenName,
    deploymentMode,
    allowEvmEmulator
}: {
    baseTokenName?: string;
    deploymentMode?: DeploymentMode;
    allowEvmEmulator?: boolean;
}): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.GOVERNOR_PRIVATE_KEY;
    let tokenMultiplierSetterAddress = process.env.TOKEN_MULTIPLIER_SETTER_ADDRESS;

    if (baseTokenName && !tokenMultiplierSetterAddress) {
        const testConfigPath = path.join(process.env.ZKSYNC_HOME as string, `etc/test_config/constant`);
        const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
        // this is one of the rich accounts
        tokenMultiplierSetterAddress = Wallet.fromMnemonic(
            process.env.MNEMONIC ?? ethTestConfig.mnemonic,
            "m/44'/60'/0'/0/2"
        ).address;
        console.log(`Defaulting token multiplier setter address to ${tokenMultiplierSetterAddress}`);
    }

    const args = [
        privateKey ? `--private-key ${privateKey}` : '',
        baseTokenName ? `--base-token-name ${baseTokenName}` : '',
        deploymentMode == DeploymentMode.Validium ? '--validium-mode' : '',
        tokenMultiplierSetterAddress ? `--token-multiplier-setter-address ${tokenMultiplierSetterAddress}` : '',
        allowEvmEmulator ? '--allow-evm-emulation' : ''
    ];
    await utils.spawn(`yarn l1-contracts register-hyperchain ${args.join(' ')} | tee registerHyperchain.log`);
    const deployLog = fs.readFileSync('registerHyperchain.log').toString();

    const l2EnvVars = ['CHAIN_ETH_ZKSYNC_NETWORK_ID', 'CONTRACTS_DIAMOND_PROXY_ADDR', 'CONTRACTS_BASE_TOKEN_ADDR'];
    console.log('Writing to', `etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`);

    const updatedContracts = updateContractsEnv(
        `etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`,
        deployLog,
        l2EnvVars
    );

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('register_hyperchain.log', updatedContracts);
}

export async function deployVerifier(): Promise<void> {
    await _deployL1(true);
}

export async function deployL1(): Promise<void> {
    await _deployL1(false);
}

async function setupLegacyBridgeEra(): Promise<void> {
    await utils.confirmAction();
    if (process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID != process.env.CONTRACTS_ERA_CHAIN_ID) {
        throw new Error('Era chain and l2 chain id do not match');
    }
    process.env.CONTRACTS_ERA_DIAMOND_PROXY_ADDR = process.env.CONTRACTS_DIAMOND_PROXY_ADDR;

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];

    await utils.spawn(`yarn l1-contracts setup-legacy-bridge-era ${args.join(' ')} | tee setupLegacyBridgeEra.log`);

    const deployLog = fs.readFileSync('setupLegacyBridgeEra.log').toString();
    const l1EnvVars = ['CONTRACTS_L1_SHARED_BRIDGE_IMPL_ADDR'];

    console.log('Writing to', `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`);
    const updatedContracts = updateContractsEnv(
        `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`,
        deployLog,
        l1EnvVars
    );

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('upgraded_shared_bridge.log', updatedContracts);
}

export const command = new Command('contract').description('contract management');

command
    .command('redeploy [deploy-opts...]')
    .allowUnknownOption(true)
    .description('redeploy contracts')
    .action(redeployL1);
command.command('deploy [deploy-opts...]').allowUnknownOption(true).description('deploy contracts').action(deployL1);
command.command('build').description('build contracts').action(build);

command.command('verify').description('verify L1 contracts').action(verifyL1Contracts);
command
    .command('setup-legacy-bridge-era')
    .description('upgrade shared bridge with deployed era diamond proxy address')
    .action(setupLegacyBridgeEra);
command
    .command('initialize-validator [init-opts...]')
    .allowUnknownOption(true)
    .description('initialize validator')
    .action(initializeValidator);
command
    .command('deploy-l2 [deploy-opts...]')
    .allowUnknownOption(true)
    .description('deploy l2 contracts')
    .action(deployL2);
command
    .command('initialize-governance [gov-opts...]')
    .allowUnknownOption(true)
    .description('initialize governance')
    .action(initializeGovernance);
command
    .command('register-hyperchain')
    .description('register hyperchain')
    .option('--base-token-name <base-token-name>', 'base token name')
    .option('--deployment-mode <deployment-mode>', 'deploy contracts in Validium mode')
    .option(
        '--token-multiplier-setter-address <token-multiplier-setter-address>',
        'address of the token multiplier setter'
    )
    .action(registerHyperchain);
command
    .command('deploy-l2-through-l1')
    .description('deploy l2 through l1')
    .option(
        '--local-legacy-bridge-testing',
        'used to test LegacyBridge compatibility. The chain will have the same id as the era chain id, while eraChainId in L2SharedBridge will be 0'
    )
    .action(deployL2ThroughL1);
command.command('deploy-verifier').description('deploy verifier to l1').action(deployVerifier);
