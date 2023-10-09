import { Command } from 'commander';
import { exec as _exec, spawn as _spawn } from 'child_process';
// import * as env from './env';
import fs from 'fs';
import { spawn, updateContractsEnv } from './utils';

// executes a command in a new shell
// but pipes data to parent's stdout/stderr

export async function build() {
    await spawn('yarn l1-contracts build');
    await spawn('yarn l2-contracts build');
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
    const updatedContracts = updateContractsEnv(deployLog, envVars);

    // Write updated contract addresses and tx hashes to the separate file
    // Currently it's used by loadtest github action to update deployment configmap.
    fs.writeFileSync('deployed_contracts.log', updatedContracts);
}

export async function redeployL1(version: ProtocolVersions, args: any[]) {
    await deployL1(version, args);
    await verifyL1Contracts(version);
}

export async function deployVerifier(version: ProtocolVersions, args: any[]) {
    await deployL1(version, [...args, '--only-verifier']);
}
