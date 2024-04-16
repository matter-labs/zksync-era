import { Command } from 'commander';
import { spawn } from 'zk/build/utils';
import fs from 'fs';
import { ethers } from 'ethers';

import { updateContractsEnv } from 'zk/build/contract';
import { getPostUpgradeCalldataFileName } from './utils';

async function hyperchainUpgrade1() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);

    await spawn(`yarn hyperchain-upgrade-1 | tee deployL1.log`);
    process.chdir(cwd);

    const deployLog = fs.readFileSync(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/deployL1.log`).toString();
    const l1EnvVars = [
        'CONTRACTS_CREATE2_FACTORY_ADDR',

        'CONTRACTS_BRIDGEHUB_PROXY_ADDR',
        'CONTRACTS_BRIDGEHUB_IMPL_ADDR',

        'CONTRACTS_STATE_TRANSITION_PROXY_ADDR',
        'CONTRACTS_STATE_TRANSITION_IMPL_ADDR',

        'CONTRACTS_DIAMOND_UPGRADE_INIT_ADDR',
        'CONTRACTS_DIAMOND_INIT_ADDR',
        'CONTRACTS_DEFAULT_UPGRADE_ADDR',
        'CONTRACTS_HYPERCHAIN_UPGRADE_ADDR',
        'CONTRACTS_GENESIS_UPGRADE_ADDR',
        'CONTRACTS_GOVERNANCE_ADDR',
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
        'CONTRACTS_BLOB_VERSIONED_HASH_RETRIEVER_ADDR'
    ];

    console.log('Writing to', `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`);
    updateContractsEnv(
        `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`,
        deployLog,
        l1EnvVars
    );
}

async function hyperchainUpgrade2() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);

    await spawn(`yarn hyperchain-upgrade-2 | tee deployL1.log`);
    process.chdir(cwd);
}

async function hyperchainUpgrade3() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);

    await spawn(`yarn hyperchain-upgrade-3 | tee deployL1.log`);
    process.chdir(cwd);
}

async function preparePostUpgradeCalldata() {
    let calldata = new ethers.utils.AbiCoder().encode(
        ['uint256', 'address', 'address', 'address'],
        [
            process.env.CONTRACTS_ERA_CHAIN_ID,
            process.env.CONTRACTS_BRIDGEHUB_PROXY_ADDR,
            process.env.CONTRACTS_STATE_TRANSITION_PROXY_ADDR,
            process.env.CONTRACTS_L1_SHARED_BRIDGE_PROXY_ADDR
        ]
    );
    let postUpgradeCalldataFileName = getPostUpgradeCalldataFileName(undefined);

    fs.writeFileSync(postUpgradeCalldataFileName, JSON.stringify(calldata, null, 2));
}

async function deploySharedBridgeL2Implementation() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l2-contracts/`);

    await spawn(`yarn deploy-shared-bridge-l2-implementation  | tee deployL1.log`);
    process.chdir(cwd);

    const deployLog = fs.readFileSync(`${process.env.ZKSYNC_HOME}/contracts/l2-contracts/deployL1.log`).toString();
    const l2EnvVars = ['CONTRACTS_L2_SHARED_BRIDGE_IMPL_ADDR'];

    console.log('Writing to', `etc/env/l2-inits/${process.env.ZKSYNC_ENV}.init.env`);
    updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, deployLog, l2EnvVars);
}

export const command = new Command('hyperchain-upgrade').description('create and publish custom l2 upgrade');

command
    .command('start')
    .description('start')
    .option('--shared-bridge-l2-implementation')
    .option('--phase1')
    .option('--post-upgrade-calldata')
    .option('--phase2')
    .option('--phase3')
    // .option('--get-diamond-upgrade-batch-number-for-eth-withdrawals')
    // .option('--get-erc20-bridge-upgrade-batch-number-for-token-withdrawals')
    // .option('--get-last-deposit-batch-number-for-failed-deposits')
    .action(async (options) => {
        if (options.phase1) {
            await hyperchainUpgrade1();
        } else if (options.phase2) {
            await hyperchainUpgrade2();
        } else if (options.postUpgradeCalldata) {
            await preparePostUpgradeCalldata();
        } else if (options.sharedBridgeL2Implementation) {
            await deploySharedBridgeL2Implementation();
        } else if (options.phase3) {
            await hyperchainUpgrade3();
        } else if (options.getDiamondUpgradeBatchNumberForEthWithdrawals) {
            // What we care about for withdrawals is the executed batch number, since we are storing isWithdrawalFinalized flag, for some valid executed txs.
            // this is printed out as part of the phase 2 script for local testing
            // for the mainnet upgrade we will have to manually check executedBatchNumber after the upgrade, since governance is signing the txs
        } else if (options.getErc20BridgeUpgradeBatchNumberForTokenWithdrawals) {
            // this is the first batchNumber after the L1ERC20Bridge has been upgraded
            // What we care about for withdrawals is the executed batch number, since we are storing isWithdrawalFinalized flag, for some valid executed txs.
            // this is printed out as part of the phase 2 script for local testing
            // for the mainnet upgrade we will have to manually check the priority queue at the given block, since governance is signing the txs
        } else if (options.getLastDepositBatchNumberForFailedDeposits) {
            // this is the batch number that the last deposit is processed in ( this is tied to a tx, so commit vs executed is not relevant)
            // we will print the priority tx queue id as part of phase 2 script for local testing, and we can use that to find the batch number
            // for the mainnet upgrade we will have to manually check the priority queue at the given block, since governance is signing the txs
        }
    });
