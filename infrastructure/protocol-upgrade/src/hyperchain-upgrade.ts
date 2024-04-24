import { Command } from 'commander';
import { spawn } from 'zk/build/utils';
import fs from 'fs';
import { ethers } from 'ethers';

import { updateContractsEnv } from 'zk/build/contract';
import * as env from 'zk/build/env';
import { setupForDal, DalPath } from 'zk/build/database';
import { getFacetsFileName, getCryptoFileName, getPostUpgradeCalldataFileName, getUpgradePath } from './utils';
import { IZkSyncHyperchainFactory } from 'l1-contracts/typechain/IZkSyncHyperchainFactory';
import { getWallet } from './transaction';

async function hyperchainUpgrade1() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);

    await spawn(`yarn hyperchain-upgrade-1 --private-key  | tee deployHyperchainUpgradeContracts.log`);
    process.chdir(cwd);

    const deployLog = fs
        .readFileSync(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/deployHyperchainUpgradeContracts.log`)
        .toString();
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

async function insertAddresses(environment?: string) {
    const facetsFile = getFacetsFileName(environment);
    const facets = JSON.parse(fs.readFileSync(facetsFile).toString());
    facets.ExecutorFacet.address = process.env.CONTRACTS_EXECUTOR_FACET_ADDR;
    facets.AdminFacet.address = process.env.CONTRACTS_ADMIN_FACET_ADDR;
    facets.GettersFacet.address = process.env.CONTRACTS_GETTERS_FACET_ADDR;
    facets.MailboxFacet.address = process.env.CONTRACTS_MAILBOX_FACET_ADDR;
    fs.writeFileSync(facetsFile, JSON.stringify(facets, null, 4));

    const verifierFile = getCryptoFileName(environment);
    const verifier = JSON.parse(fs.readFileSync(verifierFile).toString());
    verifier.verifier.address = process.env.CONTRACTS_VERIFIER_ADDR;
    fs.writeFileSync(verifierFile, JSON.stringify(verifier, null, 4));
}

async function hyperchainUpgrade2() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);
    const environment = process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : 'localhost';
    await spawn(
        `yarn hyperchain-upgrade-2 --print-file-path ${getUpgradePath(
            environment
        )} | tee deploydeployHyperchainUpgrade2.log`
    );
    process.chdir(cwd);
}

async function hyperchainUpgrade3() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);

    await spawn(`yarn hyperchain-upgrade-3 | tee deploydeployHyperchainUpgrade3.log`);
    process.chdir(cwd);
}

async function preparePostUpgradeCalldata(environment?: string) {
    let calldata = new ethers.utils.AbiCoder().encode(
        ['uint256', 'address', 'address', 'address'],
        [
            process.env.CONTRACTS_ERA_CHAIN_ID,
            process.env.CONTRACTS_BRIDGEHUB_PROXY_ADDR,
            process.env.CONTRACTS_STATE_TRANSITION_PROXY_ADDR,
            process.env.CONTRACTS_L1_SHARED_BRIDGE_PROXY_ADDR
        ]
    );
    let postUpgradeCalldataFileName = getPostUpgradeCalldataFileName(environment);

    fs.writeFileSync(postUpgradeCalldataFileName, JSON.stringify(calldata, null, 2));
}

async function deploySharedBridgeL2Implementation() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l2-contracts/`);

    await spawn(
        `yarn deploy-shared-bridge-l2-implementation  --private-key  | tee deploySharedBridgeImplementation.log`
    );
    process.chdir(cwd);

    const deployLog = fs
        .readFileSync(`${process.env.ZKSYNC_HOME}/contracts/l2-contracts/deploySharedBridgeImplementation.log`)
        .toString();
    const l2EnvVars = ['CONTRACTS_L2_SHARED_BRIDGE_IMPL_ADDR'];

    console.log('Writing to', `etc/env/l2-inits/${process.env.ZKSYNC_ENV}.init.env`);
    updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, deployLog, l2EnvVars);
}

async function hyperchainFullUpgrade() {
    // process.chdir(`${process.env.ZKSYNC_HOME}`);

    // await spawn(
    //     'cp etc/env/.init.env etc/env/l1-inits/.init.env && rm ./etc/env/l2-inits/zksync_local.init.env && rm ./etc/env/target/zksync_local.env'
    // );
    // await spawn('zk env zksync_local');
    // env.reload('zksync_local');
    // env.modify(
    //     'CONTRACTS_ERA_DIAMOND_PROXY_ADDR',
    //     process.env.CONTRACTS_DIAMOND_PROXY_ADDR,
    //     `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`
    // );
    // env.modify(
    //     'CONTRACTS_L2_SHARED_BRIDGE_ADDR',
    //     process.env.CONTRACTS_L2_ERC20_BRIDGE_ADDR,
    //     `etc/env/l2-inits/${process.env.ZKSYNC_ENV}.init.env`
    // );
    // env.modify(
    //     'CONTRACTS_BASE_TOKEN_ADDR',
    //     '0x0000000000000000000000000000000000000001',
    //     `etc/env/l2-inits/${process.env.ZKSYNC_ENV}.init.env`
    // );

    // await deploySharedBridgeL2Implementation();

    // process.chdir(`${process.env.ZKSYNC_HOME}`);
    // await spawn('zk config compile zksync_local');
    // env.reload('zksync_local');
    // // process.chdir(`${process.env.ZKSYNC_HOME}/infrastructure/protocol-upgrade`);
    // await hyperchainUpgrade1();
    // env.reload('zksync_local');

    // await insertAddresses("stage");
    env.reload('zksync_local');

    // await spawn('zk f yarn  workspace protocol-upgrade-tool start facets generate-facet-cuts --environment stage ');
    // await spawn('zk f yarn  workspace protocol-upgrade-tool start system-contracts publish-all  --environment stage --private-key ');
    // await spawn(
    //     'zk f yarn  workspace protocol-upgrade-tool start l2-transaction complex-upgrader-calldata --use-forced-deployments --use-contract-deployer --environment stage'
    // );
    // await spawn('zk f yarn  workspace protocol-upgrade-tool start crypto save-verification-params --environment stage');
    // await preparePostUpgradeCalldata("stage");
    await spawn(
        `zk f yarn  workspace protocol-upgrade-tool start transactions build-default --upgrade-timestamp 1711451944 --zksync-address ${process.env.CONTRACTS_DIAMOND_PROXY_ADDR} --use-new-governance --upgrade-address ${process.env.CONTRACTS_HYPERCHAIN_UPGRADE_ADDR} --post-upgrade-calldata --environment stage`
    );
    // await spawn(
    //     `zk f yarn  workspace protocol-upgrade-tool start transactions propose-upgrade --zksync-address ${process.env.CONTRACTS_DIAMOND_PROXY_ADDR} --new-governance ${process.env.CONTRACTS_GOVERNANCE_ADDR}`
    // );
}

async function postPropose() {
    // we need to set up the prover dal
    await setupForDal(DalPath.ProverDal, process.env.DATABASE_PROVER_URL);
    await spawn('zk db migrate');
}

export const command = new Command('hyperchain-upgrade').description('create and publish custom l2 upgrade');

command
    .command('start')
    .description('start')
    .option('--shared-bridge-l2-implementation')
    .option('--phase1')
    .option('--insert-addresses')
    .option('--post-upgrade-calldata')
    .option('--phase2')
    .option('--phase3')
    .option('--full-start')
    .option('--post-propose')
    .option('--execute-upgrade')
    .action(async (options) => {
        if (options.phase1) {
            await hyperchainUpgrade1();
        } else if (options.insertAddresses) {
            await insertAddresses();
        } else if (options.phase2) {
            const wallet = getWallet(undefined, undefined);
            const eraDiamond = IZkSyncHyperchainFactory.connect(process.env.CONTRACTS_DIAMOND_PROXY_ADDR, wallet);
            const executeBatchNumber = await eraDiamond.getTotalBatchesExecuted();
            const txId = await eraDiamond.getTotalPriorityTxs();
            console.log(`setEraPostLegacyBridgeUpgradeFirstBatch=${executeBatchNumber}`);
            console.log(`tx id necessary to calculate setEraLegacyBridgeLastDepositTime =${txId}`);
            await hyperchainUpgrade2();
        } else if (options.postUpgradeCalldata) {
            await preparePostUpgradeCalldata();
        } else if (options.sharedBridgeL2Implementation) {
            await deploySharedBridgeL2Implementation();
        } else if (options.phase3) {
            await hyperchainUpgrade3();
        } else if (options.fullStart) {
            await hyperchainFullUpgrade();
        } else if (options.postPropose) {
            await postPropose();
        } else if (options.executeUpgrade) {
            const wallet = getWallet(undefined, undefined);
            const eraDiamond = IZkSyncHyperchainFactory.connect(process.env.CONTRACTS_DIAMOND_PROXY_ADDR, wallet);
            const executeBatchNumber = await eraDiamond.getTotalBatchesExecuted();

            console.log(`setEraPostDiamondUpgradeFirstBatch=${executeBatchNumber}`);
            await spawn(
                `zk f yarn workspace protocol-upgrade-tool start transactions execute-upgrade --zksync-address ${process.env.CONTRACTS_DIAMOND_PROXY_ADDR} --new-governance ${process.env.CONTRACTS_GOVERNANCE_ADDR}`
            );
            // note we have to set 3 variables manually

            // setEraPostDiamondUpgradeFirstBatch
            // What we care about for withdrawals is the executed batch number at the moment of the diamond upgrade,
            // since we are storing isWithdrawalFinalized flag, for some valid executed txs.
            // this is printed out as part of the execute upgrade script for local testing
            // for the mainnet upgrade we will have to manually check executedBatchNumber after the upgrade, since governance is signing the txs

            // setEraPostLegacyBridgeUpgradeFirstBatch
            // this is the first batchNumber after the L1ERC20Bridge has been upgraded
            // What we care about for withdrawals is the executed batch number, since we are storing isWithdrawalFinalized flag, for some valid executed txs.
            // this is printed out as part of the phase 2 script for local testing

            // setEraLegacyBridgeLastDepositTime
            // for the mainnet upgrade we will have to manually check the priority queue at the given block, since governance is signing the txs
            // this is the batch number that the last deposit is processed in ( this is tied to a tx, so commit vs executed is not relevant)
            // we will print the priority tx queue id as part of phase 2 script for local testing, and we can use that to find the batch number
            // for the mainnet upgrade we will have to manually check the priority queue at the given block, since governance is signing the txs
        }
    });
