import { Command } from 'commander';
import { exec as _exec, spawn as _spawn } from 'child_process';
// import * as env from './env';
import fs from 'fs';

// executes a command in a new shell
// but pipes data to parent's stdout/stderr
export function spawn(command: string) {
    command = command.replace(/\n/g, ' ');
    const child = _spawn(command, { stdio: 'inherit', shell: true });
    return new Promise((resolve, reject) => {
        child.on('error', reject);
        child.on('close', (code) => {
            code == 0 ? resolve(code) : reject(`Child process exited with code ${code}`);
        });
    });
}

export async function build() {
    await spawn('yarn l1-contracts build');
    await spawn('yarn l2-contracts build');
}

function l1ContractsPath(version: ProtocolVersions): string {
    return `${contractsPathForProtocol(version)}\ethereum`;
}

function l2ContractsPath(version: ProtocolVersions): string {
    return `${contractsPathForProtocol(version)}\zksync`;
}

export async function verifyL1Contracts(protocolVersion: ProtocolVersions) {
    // Spawning a new script is expensive, so if we know that publishing is disabled, it's better to not launch
    // it at all (even though `verify` checks the network as well).
    if (process.env.CHAIN_ETH_NETWORK == 'localhost') {
        console.log('Skip contract verification on localhost');
        return;
    }
    let path = l1ContractsPath(protocolVersion);
    await spawn(`yarn --cwd ${path} verify`);
}
//
// function updateContractsEnv(deployLog: String, envVars: Array<string>) {
//     let updatedContracts = '';
//     for (const envVar of envVars) {
//         const pattern = new RegExp(`${envVar}=.*`, 'g');
//         const matches = deployLog.match(pattern);
//         if (matches !== null) {
//             const varContents = matches[0];
//             env.modify(envVar, varContents);
//             updatedContracts += `${varContents}\n`;
//         }
//     }
//
//     return updatedContracts;
// }

export async function initializeValidator(version: ProtocolVersions, args: any[] = []) {
    const baseCommandL1 = `yarn --cwd ${l1ContractsPath(version)}`;

    await spawn(`${baseCommandL1} initialize-validator ${args.join(' ')} | tee initializeValidator.log`);
}

export async function initializeL1AllowList(version: ProtocolVersions, args: any[] = []) {
    const baseCommandL1 = `yarn --cwd ${l1ContractsPath(version)}`;

    await spawn(`${baseCommandL1} initialize-allow-list ${args.join(' ')} | tee initializeL1AllowList.log`);
}

export async function initializeWethToken(version: ProtocolVersions, args: any[] = []) {
    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;
    const baseCommandL1 = `yarn --cwd ${l1ContractsPath(version)}`;

    await spawn(`${baseCommandL1} initialize-l2-weth-token instant-call ${args.join(' ')} | tee initializeWeth.log`);
}

export async function deployL2(
    version: ProtocolVersions,
    args: any[] = [],
    includePaymaster?: boolean,
    includeWETH?: boolean
) {
    const baseCommandL2 = `yarn --cwd ${l2ContractsPath(version)}`;
    const baseCommandL1 = `yarn --cwd ${l1ContractsPath(version)}`;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    await spawn(`${baseCommandL2} build`);

    await spawn(`${baseCommandL1} initialize-bridges ${args.join(' ')} | tee deployL2.log`);

    if (includePaymaster) {
        await spawn(`${baseCommandL2} deploy-testnet-paymaster ${args.join(' ')} | tee -a deployL2.log`);
    }

    if (includeWETH) {
        await spawn(`${baseCommandL2} deploy-l2-weth ${args.join(' ')} | tee -a deployL2.log`);
    }

    await spawn(`${baseCommandL2} deploy-force-deploy-upgrader ${args.join(' ')} | tee -a deployL2.log`);

    // const l2DeployLog = fs.readFileSync('deployL2.log').toString();
    // const l2DeploymentEnvVars = [
    //     'CONTRACTS_L2_ERC20_BRIDGE_ADDR',
    //     'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
    //     'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
    //     'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
    //     'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR'
    // ];
    // updateContractsEnv(l2DeployLog, l2DeploymentEnvVars);

    if (includeWETH) {
        await spawn(`${baseCommandL1} initialize-weth-bridges ${args.join(' ')} | tee -a deployL1.log`);
    }

    // const l1DeployLog = fs.readFileSync('deployL1.log').toString();
    // const l1DeploymentEnvVars = ['CONTRACTS_L2_WETH_BRIDGE_ADDR'];
    // updateContractsEnv(l1DeployLog, l1DeploymentEnvVars);
}

export async function deployL1(version: ProtocolVersions, args: any[]) {
    const baseCommand = `yarn --cwd ${l1ContractsPath(version)}`;

    await spawn(`${baseCommand} deploy-no-build ${args.join(' ')} | tee deployL1.log`);
    const deployLog = fs.readFileSync('deployL1.log').toString();
    const envVars = [
        'CONTRACTS_CREATE2_FACTORY_ADDR',
        'CONTRACTS_DIAMOND_CUT_FACET_ADDR',
        'CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR',
        'CONTRACTS_DEFAULT_UPGRADE_ADDR',
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
        'CONTRACTS_L1_WETH_BRIDGE_IMPL_ADDR',
        'CONTRACTS_L1_WETH_BRIDGE_PROXY_ADDR',
        'CONTRACTS_L1_ALLOW_LIST_ADDR',
        'CONTRACTS_L1_MULTICALL3_ADDR'
    ];
    // const updatedContracts = updateContractsEnv(deployLog, envVars);
    //
    // // Write updated contract addresses and tx hashes to the separate file
    // // Currently it's used by loadtest github action to update deployment configmap.
    // fs.writeFileSync('deployed_contracts.log', updatedContracts);
}

export async function redeployL1(version: ProtocolVersions, args: any[]) {
    await deployL1(version, args);
    await verifyL1Contracts(version);
}

export async function deployVerifier(version: ProtocolVersions, args: any[]) {
    await deployL1(version, [...args, '--only-verifier']);
}

export const command = new Command('contract').option('--protocol-version').description('contract management');

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
