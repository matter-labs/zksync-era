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
    await utils.spawn(`yarn l1-contracts initialize-validator ${args.join(' ')} | tee initializeValidator.log`);
}

export async function initializeGovernance(args: any[] = []) {
    await utils.confirmAction();
    await utils.spawn(`yarn l1-contracts initialize-governance ${args.join(' ')} | tee initializeGovernance.log`);
}

export async function initializeWethToken(args: any[] = []) {
    await utils.confirmAction();
    await utils.spawn(
        `yarn l1-contracts initialize-l2-weth-token instant-call ${args.join(' ')} | tee initializeWeth.log`
    );
}

export async function deployL2(args: any[] = [], includePaymaster?: boolean, includeWETH?: boolean) {
    await utils.confirmAction();

    const isLocalSetup = process.env.ZKSYNC_LOCAL_SETUP;

    // Skip compilation for local setup, since we already copied artifacts into the container.
    if (!isLocalSetup) {
        await utils.spawn(`yarn l2-contracts build`);
    }


    await utils.spawn(`yarn l1-contracts initialize-bridges ${args.join(' ')} | tee deployL2.log`);

    if (includePaymaster) {
        await utils.spawn(`yarn l2-contracts deploy-testnet-paymaster ${args.join(' ')} | tee -a deployL2.log`);
    }

    if (includeWETH) {
        await utils.spawn(`yarn l2-contracts deploy-l2-weth ${args.join(' ')} | tee -a deployL2.log`);
    }

    await utils.spawn(`yarn l2-contracts deploy-force-deploy-upgrader ${args.join(' ')} | tee -a deployL2.log`);

    const l2DeployLog = fs.readFileSync('deployL2.log').toString();
    const l2DeploymentEnvVars = [
        'CONTRACTS_L2_ERC20_BRIDGE_ADDR',
        'CONTRACTS_L2_TESTNET_PAYMASTER_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_IMPL_ADDR',
        'CONTRACTS_L2_WETH_TOKEN_PROXY_ADDR',
        'CONTRACTS_L2_DEFAULT_UPGRADE_ADDR'
    ];
    updateContractsEnv(l2DeployLog, l2DeploymentEnvVars);

    if (includeWETH) {
        await utils.spawn(`yarn l1-contracts initialize-weth-bridges ${args.join(' ')} | tee -a deployL1.log`);
    }

    const l1DeployLog = fs.readFileSync('deployL1.log').toString();
    const l1DeploymentEnvVars = ['CONTRACTS_L2_WETH_BRIDGE_ADDR'];
    updateContractsEnv(l1DeployLog, l1DeploymentEnvVars);
}

export async function deployL1(args: any[]) {
    await utils.confirmAction();
    await utils.spawn(`yarn l1-contracts deploy-no-build ${args.join(' ')} | tee deployL1.log`);
    const deployLog = fs.readFileSync('deployL1.log').toString();
    const envVars = [
        'CONTRACTS_CREATE2_FACTORY_ADDR',
        'CONTRACTS_ADMIN_FACET_ADDR',
        'CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR',
        'CONTRACTS_DEFAULT_UPGRADE_ADDR',
        'CONTRACTS_GOVERNANCE_ADDR',
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
        'CONTRACTS_L1_MULTICALL3_ADDR',
        'CONTRACTS_BLOB_VERSIONED_HASH_RETRIEVER_ADDR'
    ];
    const updatedContracts = updateContractsEnv(deployLog, envVars);

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('deployed_contracts.log', updatedContracts);
}

export enum DeploymentMode {
    Rollup = 0,
    Validium = 1
}

export async function redeployL1(args: any[], deploymentMode: DeploymentMode) {
    if (deploymentMode == DeploymentMode.Validium) {
        await deployL1([...args, '--validium-mode']);
    } else if (deploymentMode == DeploymentMode.Rollup) {
        await deployL1(args);
    } else {
        throw new Error('Invalid deployment mode');
    }
    await verifyL1Contracts();
}

export async function deployVerifier(args: any[], deploymentMode: DeploymentMode) {
    if (deploymentMode == DeploymentMode.Validium) {
        await deployL1([...args, '--only-verifier', '--validium-mode']);
    } else if (deploymentMode == DeploymentMode.Rollup) {
        await deployL1([...args, '--only-verifier']);
    } else {
        throw new Error('Invalid deployment mode');
    }
}

export const command = new Command('contract').description('contract management');

command
    .command('redeploy [deploy-opts...]')
    .allowUnknownOption(true)
    .description('redeploy contracts')
    .action(redeployL1);
command.command('deploy [deploy-opts...]').allowUnknownOption(true).description('deploy contracts').action(deployL1);
command.command('build').description('build contracts').action(build);
command.command('initialize-validator [init-opts...]').allowUnknownOption(true).description('initialize validator').action(initializeValidator);
command.command('verify').description('verify L1 contracts').action(verifyL1Contracts);
command.command('deploy-l2 [deploy-opts...]').allowUnknownOption(true).description('deploy l2 contracts').action(deployL2);
command.command('initialize-governance [gov-opts...]').allowUnknownOption(true).description('initialize governance').action(initializeGovernance);
