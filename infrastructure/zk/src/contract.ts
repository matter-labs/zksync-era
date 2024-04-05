import { Command } from 'commander';
import * as utils from './utils';
import * as env from './env';
import fs from 'fs';

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

export async function initializeGovernance(): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.GOVERNANCE_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/l1-contracts` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommandL1} initialize-governance ${args.join(' ')} | tee initializeGovernance.log`);
}

export async function initializeL1AllowList(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommandL1} initialize-allow-list ${args.join(' ')} | tee initializeL1AllowList.log`);
}

export async function deployWeth(
    command: 'dev' | 'new',
    name?: string,
    symbol?: string,
    decimals?: string,
    args: any = []
): Promise<void> {
    // let destinationFile = 'localhost';
    // if (args.includes('--envFile')) {
    //     destinationFile = args[args.indexOf('--envFile') + 1];
    //     args.splice(args.indexOf('--envFile'), 2);
    // }
    await utils.spawn(`yarn --silent --cwd contracts/l1-contracts deploy-weth '
            ${args.join(' ')} | tee deployL1.log`);

    const deployLog = fs.readFileSync('deployL1.log').toString();
    const l1DeploymentEnvVars = ['CONTRACTS_L1_WETH_TOKEN_ADDR'];
    updateContractsEnv(
        `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`,
        deployLog,
        l1DeploymentEnvVars
    );
}

export async function deployL2(args: any[] = [], includePaymaster?: boolean): Promise<void> {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommandL2 = isLocalSetup ? `yarn --cwd /contracts/l2-contracts` : `yarn l2-contracts`;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    await utils.spawn(`${baseCommandL2} build`);

    await utils.spawn(`${baseCommandL2} deploy-shared-bridge-on-l2 ${args.join(' ')} | tee deployL2.log`);

    if (includePaymaster) {
        await utils.spawn(`${baseCommandL2} deploy-testnet-paymaster ${args.join(' ')} | tee -a deployL2.log`);
    }

    await utils.spawn(`${baseCommandL2} deploy-force-deploy-upgrader ${args.join(' ')} | tee -a deployL2.log`);

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
export async function deployL2ThroughL1({ includePaymaster }: { includePaymaster: boolean }): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [privateKey ? `--private-key ${privateKey}` : ''];

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommandL2 = isLocalSetup ? `yarn --cwd /contracts/l2-contracts` : `yarn l2-contracts`;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    await utils.spawn(`${baseCommandL2} build`);

    await utils.spawn(`${baseCommandL2} deploy-shared-bridge-on-l2-through-l1 ${args.join(' ')} | tee deployL2.log`);

    if (includePaymaster) {
        await utils.spawn(
            `${baseCommandL2} deploy-testnet-paymaster-through-l1 ${args.join(' ')} | tee -a deployL2.log`
        );
    }

    await utils.spawn(
        `${baseCommandL2} deploy-force-deploy-upgrader-through-l1 ${args.join(' ')} | tee -a deployL2.log`
    );

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

async function _deployL1(onlyVerifier: boolean, deploymentMode: DeploymentMode): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const args = [
        privateKey ? `--private-key ${privateKey}` : '',
        onlyVerifier ? '--only-verifier' : '',
        deploymentMode == DeploymentMode.Validium ? '--validium' : ''
    ];

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommand = process.env.ZKSYNC_LOCAL_SETUP ? `yarn --cwd /contracts/l1-contracts` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommand} deploy-no-build ${args.join(' ')} | tee deployL1.log`);
    const deployLog = fs.readFileSync('deployL1.log').toString();
    const l1EnvVars = [
        'CONTRACTS_CREATE2_FACTORY_ADDR',

        'CONTRACTS_BRIDGEHUB_PROXY_ADDR',
        'CONTRACTS_BRIDGEHUB_IMPL_ADDR',

        'CONTRACTS_STATE_TRANSITION_PROXY_ADDR',
        'CONTRACTS_STATE_TRANSITION_IMPL_ADDR',

        'CONTRACTS_ADMIN_FACET_ADDR',
        'CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR',
        'CONTRACTS_DIAMOND_INIT_ADDR',
        'CONTRACTS_DEFAULT_UPGRADE_ADDR',
        'CONTRACTS_GENESIS_UPGRADE_ADDR',
        'CONTRACTS_GOVERNANCE_ADDR',
        'CONTRACTS_MAILBOX_FACET_ADDR',
        'CONTRACTS_EXECUTOR_FACET_ADDR',
        'CONTRACTS_GETTERS_FACET_ADDR',

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
        'CONTRACTS_BLOB_VERSIONED_HASH_RETRIEVER_ADDR'
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

export async function redeployL1(verifierOnly: boolean, deploymentMode: DeploymentMode) {
    await _deployL1(verifierOnly, deploymentMode);
    await verifyL1Contracts();
}

export async function wethBridgeFinish(args: any[] = []): Promise<void> {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommandL1} weth-finish-deployment-on-chain ${args.join(' ')} | tee -a deployL2.log`);
}

export async function erc20BridgeFinish(args: any[] = []): Promise<void> {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommandL1} erc20-finish-deployment-on-chain ${args.join(' ')} | tee -a deployL2.log`);
}

export async function registerHyperchain({ baseTokenName }: { baseTokenName?: string }): Promise<void> {
    await utils.confirmAction();

    const privateKey = process.env.GOVERNOR_PRIVATE_KEY;
    const args = [
        privateKey ? `--private-key ${privateKey}` : '',
        baseTokenName ? `--base-token-name ${baseTokenName}` : ''
    ];

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommand = process.env.ZKSYNC_LOCAL_SETUP ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommand} register-hyperchain ${args.join(' ')} | tee registerHyperchain.log`);
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
    // Deploy mode doesn't matter here
    await _deployL1(true, DeploymentMode.Rollup);
}

export async function deployL1(args: [string]): Promise<void> {
    let mode;
    if (args.includes('validium')) {
        mode = DeploymentMode.Validium;
    } else {
        mode = DeploymentMode.Rollup;
    }
    await _deployL1(false, mode);
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
