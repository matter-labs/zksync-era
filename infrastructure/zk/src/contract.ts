import { Command } from 'commander';
import * as utils from './utils';
import * as env from './env';
import fs from 'fs';

export async function build() {
    await utils.spawn('yarn l1-contracts build');
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

function updateContractsEnv(deployLog: String, envVars: Array<string>) {
    let updatedContracts = '';
    for (const envVar of envVars) {
        const pattern = new RegExp(`${envVar}=.*`, 'g');
        const matches = deployLog.match(pattern);
        if (matches !== null) {
            const varContents = matches[0];
            env.modify(envVar, varContents);
            updatedContracts += `${varContents}\n`;
        }
    }

    return updatedContracts;
}

export async function initializeValidator(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommandL1} initialize-validator ${args.join(' ')} | tee initilizeValidator.log`);
}

export async function initializeL1AllowList(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommandL1} initialize-allow-list ${args.join(' ')} | tee initilizeL1AllowList.log`);
}

export async function deployL2(args: any[] = []) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommandL2 = isLocalSetup ? `yarn --cwd /contracts/zksync` : `yarn l2-contracts`;
    const baseCommandL1 = isLocalSetup ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    await utils.spawn(`${baseCommandL2} build`);
    await utils.spawn(`${baseCommandL2} compile-and-deploy-libs ${args.join(' ')}`);

    // IMPORTANT: initialize-bridges must go strictly *right after* the compile-and-deploy-libs step.
    // Otherwise, the ExternalDecoder library will be erased.
    await utils.spawn(`${baseCommandL1} initialize-bridges ${args.join(' ')} | tee deployL2.log`);

    await utils.spawn(`${baseCommandL2} deploy-testnet-paymaster ${args.join(' ')} | tee -a deployL2.log`);

    await utils.spawn(`${baseCommandL2} deploy-l2-weth ${args.join(' ')} | tee -a deployL2.log`);

    const deployLog = fs.readFileSync('deployL2.log').toString();
    const envVars = [
        'CONTRACTS_L2_ETH_BRIDGE_ADDR',
        'CONTRACTS_L2_ERC20_BRIDGE_ADDR',
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_IMPLEMENTATION_ADDR',
        'CONTRACTS_L2_WETH_PROXY_ADDR'
    ];

    updateContractsEnv(deployLog, envVars);
}

export async function deployL1(args: any[]) {
    await utils.confirmAction();

    // In the localhost setup scenario we don't have the workspace,
    // so we have to `--cwd` into the required directory.
    const baseCommand = process.env.ZKSYNC_LOCAL_SETUP ? `yarn --cwd /contracts/ethereum` : `yarn l1-contracts`;

    await utils.spawn(`${baseCommand} deploy-no-build ${args.join(' ')} | tee deployL1.log`);
    const deployLog = fs.readFileSync('deployL1.log').toString();
    const envVars = [
        'CONTRACTS_DIAMOND_CUT_FACET_ADDR',
        'CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR',
        'CONTRACTS_GOVERNANCE_FACET_ADDR',
        'CONTRACTS_MAILBOX_FACET_ADDR',
        'CONTRACTS_EXECUTOR_FACET_ADDR',
        'CONTRACTS_GETTERS_FACET_ADDR',
        'CONTRACTS_VERIFIER_ADDR',
        'CONTRACTS_DIAMOND_INIT_ADDR',
        'CONTRACTS_DIAMOND_PROXY_ADDR',
        'CONTRACTS_VALIDATOR_TIMELOCK_ADDR',
        'CONTRACTS_GENESIS_TX_HASH',
        'CONTRACTS_L1_ERC20_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ERC20_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_ALLOW_LIST_ADDR'
    ];
    const updatedContracts = updateContractsEnv(deployLog, envVars);

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('deployed_contracts.log', updatedContracts);
}

export async function redeployL1(args: any[]) {
    await deployL1(args);
    await verifyL1Contracts();
}

export const command = new Command('contract').description('contract management');

command
    .command('redeploy [deploy-opts...]')
    .allowUnknownOption(true)
    .description('redeploy contracts')
    .action(redeployL1);
command.command('deploy [deploy-opts...]').allowUnknownOption(true).description('deploy contracts').action(deployL1);
command.command('build').description('build contracts').action(build);
command.command('initilize-validator').description('initialize validator').action(initializeValidator);
command
    .command('initilize-l1-allow-list-contract')
    .description('initialize L1 allow list contract')
    .action(initializeL1AllowList);
command.command('verify').description('verify L1 contracts').action(verifyL1Contracts);
