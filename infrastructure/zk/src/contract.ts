import { Command } from 'commander';
import * as utils from './utils';
import * as env from './env';
import fs from 'fs';

export async function build() {
    await utils.spawn('yarn l1-contracts build');
    await utils.spawn('yarn l2-contracts build');
}

export async function verifyL1Contracts() {
    // Spawning a new script is expensive, so if we know that publishing is disabled, it's better to not launch
    // it at all (even though `verify` checks the network as well).
    if (process.env.CHAIN_ETH_NETWORK == 'localhost') {
        console.log('Skip contract verification on localhost');
        return;
    }
    await utils.spawn('yarn l1-contracts verify');
}

function updateContractsEnv(initEnv: string, deployLog: String, envVars: Array<string>) {
    let updatedContracts = '';
    for (const envVar of envVars) {
        const pattern = new RegExp(`${envVar}=.*`, 'g');
        const matches = deployLog.match(pattern);
        if (matches !== null) {
            const varContents = matches[0];
            env.modify(envVar, varContents, initEnv);
            updatedContracts += `${varContents}\n`;
        }
    }

    return updatedContracts;
}

export async function initializeValidator(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    const governorPrivateKey = process.env.GOVERNOR_PRIVATE_KEY;
    if (governorPrivateKey) {
        args.push('--private-key', governorPrivateKey);
    }

    await utils.spawn(`${baseCommandL1} initialize-validator ${args.join(' ')} | tee initializeValidator.log`);
}

export async function initializeGovernance(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommandL1} initialize-governance ${args.join(' ')} | tee initializeGovernance.log`);
}

export async function initializeGovernanceChain(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommandL1} initialize-governance-chain ${args.join(' ')} | tee initializeGovernance.log`);
}

export async function initializeL1AllowList(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    const governorPrivateKey = process.env.GOVERNOR_PRIVATE_KEY;
    if (governorPrivateKey) {
        args.push('--private-key', governorPrivateKey);
    }

    await utils.spawn(`${baseCommandL1} initialize-allow-list ${args.join(' ')} | tee initializeL1AllowList.log`);
}

export async function initializeWethToken(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(
        `${baseCommandL1} initialize-l2-weth-token instant-call ${args.join(' ')} | tee initializeWeth.log`
    );
}

export async function initializeBridges(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommandL1} initialize-bridges ${args.join(' ')} | tee deployL1.log`);
    const l2DeploymentEnvVars: string[] = [
        'CONTRACTS_L2_WETH_BRIDGE_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR'
    ];
    const l1DeployLog = fs.readFileSync('deployL1.log').toString();
    updateContractsEnv(`etc/env/l1-inits/${process.env.ZKSYNC_ENV!}.init.env`, l1DeployLog, l2DeploymentEnvVars);
}

export async function deployL2(args: any[] = [], includePaymaster?: boolean, includeWETH?: boolean) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    const deployerPrivateKey = process.env.DEPLOYER_PRIVATE_KEY;
    if (deployerPrivateKey) {
        args.push('--private-key', deployerPrivateKey);
    }

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommandL2 = isLocalSetup ? `yarn --cwd /contracts/zksync` : `yarn l2-contracts`;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    await utils.spawn(`${baseCommandL2} build`);

    await utils.spawn(`${baseCommandL1} initialize-erc20-bridge-chain ${args.join(' ')} | tee deployL2.log`);

    if (includePaymaster) {
        await utils.spawn(`${baseCommandL2} deploy-testnet-paymaster ${args.join(' ')} | tee -a deployL2.log`);
    }

    if (includeWETH) {
        await utils.spawn(`${baseCommandL1} initialize-weth-bridge-chain ${args.join(' ')} | tee -a deployL2.log`);
    }

    await utils.spawn(`${baseCommandL2} deploy-force-deploy-upgrader ${args.join(' ')} | tee -a deployL2.log`);

    let l2DeployLog = fs.readFileSync('deployL2.log').toString();
    const l2DeploymentEnvVars = [
        'CONTRACTS_L2_ERC20_BRIDGE_ADDR',
        'CONTRACTS_L2_WETH_BRIDGE_ADDR',
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
        'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR'
    ];
    updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, l2DeployLog, l2DeploymentEnvVars);
}

export async function deployL1(args: any[]) {
    await utils.confirmAction();

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommand = process.env.ZKSYNC_LOCAL_SETUP ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommand} deploy-no-build ${args.join(' ')} | tee deployL1.log`);
    const deployLog = fs.readFileSync('deployL1.log').toString();
    const l1EnvVars = [
        'CONTRACTS_CREATE2_FACTORY_ADDR',

        'CONTRACTS_BRIDGEHUB_DIAMOND_PROXY_ADDR',
        'CONTRACTS_BRIDGEHUB_ADMIN_FACET_ADDR',
        'CONTRACTS_BRIDGEHUB_MAILBOX_FACET_ADDR',
        'CONTRACTS_BRIDGEHUB_GETTERS_FACET_ADDR',
        'CONTRACTS_BRIDGEHUB_DIAMOND_INIT_ADDR',

        'CONTRACTS_STATE_TRANSITION_PROXY_ADDR',
        'CONTRACTS_STATE_TRANSITION_IMPL_ADDR',
        'CONTRACTS_STATE_TRANSITION_PROXY_ADMIN_ADDR',

        'CONTRACTS_ADMIN_FACET_ADDR',
        'CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR',
        'CONTRACTS_DIAMOND_INIT_ADDR',
        'CONTRACTS_DEFAULT_UPGRADE_ADDR',
        'CONTRACTS_GOVERNANCE_ADDR',
        'CONTRACTS_MAILBOX_FACET_ADDR',
        'CONTRACTS_EXECUTOR_FACET_ADDR',
        'CONTRACTS_GETTERS_FACET_ADDR',

        'CONTRACTS_VERIFIER_ADDR',

        'CONTRACTS_GENESIS_TX_HASH',
        'CONTRACTS_L1_ERC20_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ERC20_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_WETH_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_WETH_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ALLOW_LIST_ADDR',
        'CONTRACTS_L1_MULTICALL3_ADDR'
    ];

    console.log('Writing to', 'etc/env/l1-inits/.init.env');
    const updatedContracts = updateContractsEnv('etc/env/l1-inits/.init.env', deployLog, l1EnvVars);

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('deployed_contracts.log', updatedContracts);
}

export async function redeployL1(args: any[]) {
    const deployerPrivateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const governorAddress = process.env.GOVERNOR_ADDRESS;

    if (deployerPrivateKey && governorAddress) {
        args.concat(['--private-key', deployerPrivateKey, '--governor-address', governorAddress]);
    }

    await deployL1(args);
    await verifyL1Contracts();
}

export async function registerHyperchain(args: any[]) {
    const deployerPrivateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const governorAddress = process.env.GOVERNOR_ADDRESS;

    if (deployerPrivateKey && governorAddress) {
        args.concat(['--private-key', deployerPrivateKey, '--governor-address', governorAddress]);
    }

    await utils.confirmAction();

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommand = process.env.ZKSYNC_LOCAL_SETUP ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommand} register-hyperchain ${args.join(' ')} | tee registerHyperchain.log`);
    const deployLog = fs.readFileSync('registerHyperchain.log').toString();

    const l2EnvVars = [
        'CHAIN_ETH_ZKSYNC_NETWORK_ID',
        'CONTRACTS_DIAMOND_PROXY_ADDR',
        'CONTRACTS_VALIDATOR_TIMELOCK_ADDR'
    ];
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

export async function deployVerifier(args: any[]) {
    await deployL1([...args, '--only-verifier']);
}

export const command = new Command('contract').description('contract management');

command
    .command('redeploy [deploy-opts...]')
    .allowUnknownOption(true)
    .description('redeploy contracts')
    .action(redeployL1);
command.command('deploy [deploy-opts...]').allowUnknownOption(true).description('deploy contracts').action(deployL1);
command.command('build').description('build contracts').action(build);
command.command('initialize-validator').description('initialize validator').action(initializeValidator);
command
    .command('initialize-l1-allow-list-contract')
    .description('initialize L1 allow list contract')
    .action(initializeL1AllowList);
command.command('verify').description('verify L1 contracts').action(verifyL1Contracts);
