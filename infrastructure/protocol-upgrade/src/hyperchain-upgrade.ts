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

let privateKey = '';

async function hyperchainUpgrade1() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);

    await spawn(`yarn hyperchain-upgrade-1 --private-key ${privateKey} | tee deployHyperchainUpgradeContracts.log`);
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
    const environment = 'stage'; //process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : 'localhost';
    await spawn(
        `yarn hyperchain-upgrade-2 --print-file-path ${getUpgradePath(
            environment
        )} --private-key  ${privateKey} --gas-price 200 | tee deploydeployHyperchainUpgrade2.log`
    );
    process.chdir(cwd);
}

async function hyperchainUpgradeValidators() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);
    const environment = 'stage'; //process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : 'localhost';
    await spawn(
        `yarn hyperchain-upgrade-validator --print-file-path ${getUpgradePath(
            environment
        )} --private-key  ${privateKey} | tee deploydeployHyperchainUpgradeValidator.log`
    );
    process.chdir(cwd);
}

async function hyperchainUpgrade3() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);
    const environment = 'stage'; //process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : 'localhost';
    await spawn(
        `yarn hyperchain-upgrade-3 --print-file-path ${getUpgradePath(
            environment
        )} --private-key  ${privateKey} | tee deploydeployHyperchainUpgrade3.log`
    );
    process.chdir(cwd);
}

async function whatever() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);
    const environment = 'stage'; //process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : 'localhost';
    await spawn(
        `yarn whatever --print-file-path ${getUpgradePath(
            environment
        )} --private-key  ${privateKey} | tee deployWhatever.log`
    );
    process.chdir(cwd);
}

async function whatever2() {
    // const cwd = process.cwd();
    // process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l1-contracts/`);
    // const environment = 'stage'; //process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : 'localhost';
    // for (let i= 4; i< 34; i++ ) {
    //     let calldata1 = scheduleMigrationArray[i];
    //     let calldata2 = executeArray[i];
    // await spawn(
    //     `cast send  0x0b622A2061EaccAE1c664eBC3E868b8438e03F61 --from 0x4e4943346848c4867F81dFb37c4cA9C5715A7828 --unlocked ${calldata1}`
    // );
    // await spawn(
    //     `cast send 0x0b622A2061EaccAE1c664eBC3E868b8438e03F61 --from 0x4e4943346848c4867F81dFb37c4cA9C5715A7828 --unlocked ${calldata2}`
    // );
    // }

    // await spawn(`cast abi-encode ${upgradeData}`)

    // process.chdir(cwd);

    const gnosisabi = ['function multiSend(bytes memory transactions) public payable'];

    const gnosisinterface = new ethers.utils.Interface(gnosisabi);
    const res = gnosisinterface.parseTransaction({
        // Insert full raw data here
        data: '0x8d80ff0a00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000812000b622a2061eaccae1c664ebc3e868b8438e03f61000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003c42c43191700000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000007f31345fc6e2cb84ffcfd8c3fc10530c1ef2ee711267934993f1ee696c42ecab0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000032400084c286cf3e17e7b677ea9583e60a0003240000000000000000000000000000000000000000000000000654099584706c0000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000244eb67241900000000000000000000000011f943b2c77b743ab90f4a0ae7d5a4e7fca3e102000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000044aa20000000000000000000000000000000000000000000000000000000000000003200000000000000000000000000000000000000000000000000000000000000220000000000000000000000000343ee72ddd8ccd80cd43d6adbc6c463a2de433a700000000000000000000000000000000000000000000000000000000000001044f1ef286000000000000000000000000470afaacce2acdaefcc662419b74c79d76c914ae00000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000084a31ee5b0000000000000000000000000d7f9f54194c633f36ccd5f3da84ad4a1c38cb2cb00000000000000000000000057891966931eb4bb6fb81430e6ce0a03aabde063010001211b0c33353cdf7a320f768e3dc40bce1326d639fcac099bba9ecd8e340000000000000000000000001c732a2061eaccae1c664ebc3e868b8438e050720000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b622a2061eaccae1c664ebc3e868b8438e03f61000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003a474da756b0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000007f31345fc6e2cb84ffcfd8c3fc10530c1ef2ee711267934993f1ee696c42ecab0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000032400084c286cf3e17e7b677ea9583e60a0003240000000000000000000000000000000000000000000000000654099584706c0000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000244eb67241900000000000000000000000011f943b2c77b743ab90f4a0ae7d5a4e7fca3e102000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000044aa20000000000000000000000000000000000000000000000000000000000000003200000000000000000000000000000000000000000000000000000000000000220000000000000000000000000343ee72ddd8ccd80cd43d6adbc6c463a2de433a700000000000000000000000000000000000000000000000000000000000001044f1ef286000000000000000000000000470afaacce2acdaefcc662419b74c79d76c914ae00000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000084a31ee5b0000000000000000000000000d7f9f54194c633f36ccd5f3da84ad4a1c38cb2cb00000000000000000000000057891966931eb4bb6fb81430e6ce0a03aabde063010001211b0c33353cdf7a320f768e3dc40bce1326d639fcac099bba9ecd8e340000000000000000000000001c732a2061eaccae1c664ebc3e868b8438e0507200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000'
    });
    const totalData = res.args[0];
    console.log(totalData.length);

    const ans = [];

    let i = 2;
    while (i < totalData.length) {
        const type = totalData.slice(i, i + 2);
        i += 2;
        const to = totalData.slice(i, i + 40);
        // console.log(to);
        i += 40;
        const value = totalData.slice(i, i + 64);
        // console.log(value);
        i += 64;
        // console.log('0x' + totalData.slice(i, i + 64));
        const dlen = ethers.BigNumber.from('0x' + totalData.slice(i, i + 64)).toNumber();
        i += 64;
        // console.log(dlen);
        const data = totalData.slice(i, i + 2 * dlen);
        // console.log(data);
        i += dlen * 2;

        ans.push({
            type,
            to,
            value,
            data
        });
    }
    console.log(ans);
    return;
}

async function preparePostUpgradeCalldata(environment?: string) {
    let calldata = new ethers.utils.AbiCoder().encode(
        ['uint256', 'address', 'address', 'address', 'address', 'address'],
        [
            process.env.CONTRACTS_ERA_CHAIN_ID,
            process.env.CONTRACTS_BRIDGEHUB_PROXY_ADDR,
            process.env.CONTRACTS_STATE_TRANSITION_PROXY_ADDR,
            process.env.CONTRACTS_L1_SHARED_BRIDGE_PROXY_ADDR,
            process.env.CONTRACTS_GOVERNANCE_ADDR,
            process.env.CONTRACTS_VALIDATOR_TIMELOCK_ADDR
        ]
    );
    let postUpgradeCalldataFileName = getPostUpgradeCalldataFileName(environment);

    fs.writeFileSync(postUpgradeCalldataFileName, JSON.stringify(calldata, null, 2));
}

async function deploySharedBridgeL2Implementation() {
    const cwd = process.cwd();
    process.chdir(`${process.env.ZKSYNC_HOME}/contracts/l2-contracts/`);

    await spawn(
        `yarn deploy-shared-bridge-l2-implementation  --private-key ${privateKey} | tee deploySharedBridgeImplementation.log`
    );
    process.chdir(cwd);

    // const deployLog = fs
    //     .readFileSync(`${process.env.ZKSYNC_HOME}/contracts/l2-contracts/deploySharedBridgeImplementation.log`)
    //     .toString();
    // const l2EnvVars = ['CONTRACTS_L2_SHARED_BRIDGE_IMPL_ADDR'];

    // console.log('Writing to', `etc/env/l2-inits/${process.env.ZKSYNC_ENV}.init.env`);
    // updateContractsEnv(`etc/env/l2-inits/${process.env.ZKSYNC_ENV!}.init.env`, deployLog, l2EnvVars);
}

async function hyperchainFullUpgrade() {
    const environment = 'mainnet';
    await insertAddresses(environment);
    env.reload(environment);

    await spawn(
        `zk f yarn  workspace protocol-upgrade-tool start facets generate-facet-cuts --environment ${environment}`
    );
    await spawn(
        `zk f yarn  workspace protocol-upgrade-tool start system-contracts publish-all  --environment ${environment} --private-key ${privateKey}`
    );
    await spawn(
        `zk f yarn  workspace protocol-upgrade-tool start l2-transaction complex-upgrader-calldata --use-forced-deployments --use-contract-deployer --environment ${environment}`
    );
    await spawn(
        `zk f yarn  workspace protocol-upgrade-tool start crypto save-verification-params --environment ${environment}`
    );
    await preparePostUpgradeCalldata(environment);
    await spawn(
        `zk f yarn  workspace protocol-upgrade-tool start transactions build-default --upgrade-timestamp 1717653600 --zksync-address ${process.env.CONTRACTS_DIAMOND_PROXY_ADDR} --use-new-governance --upgrade-address ${process.env.CONTRACTS_HYPERCHAIN_UPGRADE_ADDR} --post-upgrade-calldata --environment ${environment}`
    );
}

async function hyperchainProverFixUpgrade() {
    const environment = 'testnet-fix2';
    await insertAddresses(environment);
    await spawn(
        `zk f yarn  workspace protocol-upgrade-tool start facets generate-facet-cuts --environment ${environment} `
    );
    await spawn(
        `zk f yarn  workspace protocol-upgrade-tool start crypto save-verification-params --environment ${environment}`
    );
    await spawn(
        `zk f yarn  workspace protocol-upgrade-tool start transactions build-default --upgrade-timestamp 1711451944 --zksync-address ${process.env.CONTRACTS_DIAMOND_PROXY_ADDR} --chain-id ${process.env.CONTRACTS_ERA_CHAIN_ID} --stm-address ${process.env.CONTRACTS_STATE_TRANSITION_PROXY_ADDR} --old-protocol-version 24 --old-protocol-version-deadline 0 --new-protocol-version 24 --use-new-governance --environment ${environment}`
    );
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
    .option('--add-validators')
    .option('--hyperchain-prover-fix-upgrade')
    .option('--whatever')
    .option('--whatever2')
    .option('--private-key <private-key>')
    .action(async (options) => {
        privateKey = options.privateKey;
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
        } else if (options.addValidators) {
            await hyperchainUpgradeValidators();
        } else if (options.hyperchainProverFixUpgrade) {
            await hyperchainProverFixUpgrade();
        } else if (options.whatever) {
            await whatever();
        } else if (options.whatever2) {
            await whatever2();
        }
    });
