// Test of the behaviour of the external node when L1 batches get reverted.
//
// NOTE:
// main_contract.getTotalBatchesCommitted actually checks the number of batches committed.
// main_contract.getTotalBatchesExecuted actually checks the number of batches executed.
import * as utils from 'utils';
import {
    checkRandomTransfer,
    executeDepositAfterRevert,
    executeRevert,
    Node,
    NodeSpawner,
    NodeType,
    waitToCommitBatchesWithoutExecution,
    waitToExecuteBatch
} from './utils';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { assert, expect } from 'chai';
import fs from 'node:fs/promises';
import * as child_process from 'child_process';
import * as dotenv from 'dotenv';
import { loadConfig, replaceAggregatedBlockExecuteDeadline, shouldLoadConfigFromFile } from 'utils/build/file-configs';
import path from 'path';
import { logsTestPath } from 'utils/build/logs';
import { IZkSyncHyperchain } from 'zksync-ethers/build/typechain';

const pathToHome = path.join(__dirname, '../../../..');
const fileConfig = shouldLoadConfigFromFile();

async function logsPath(name: string): Promise<string> {
    return await logsTestPath(fileConfig.chain, 'logs/revert/en', name);
}

function run(cmd: string, args: string[], options: child_process.SpawnOptions): child_process.SpawnSyncReturns<Buffer> {
    let res = child_process.spawnSync(cmd, args, options);
    expect(res.error).to.be.undefined;
    return res;
}

function compileBinaries() {
    console.log('compiling binaries');
    run(
        'cargo',
        ['build', '--release', '--bin', 'zksync_external_node', '--bin', 'zksync_server', '--bin', 'block_reverter'],
        { cwd: process.env.ZKSYNC_HOME }
    );
}

// Fetches env vars for the given environment (like 'dev', 'ext-node').
// TODO: it would be better to import zk tool code directly.
function fetchEnv(zksyncEnv: string): Record<string, string | undefined> {
    let res = run('./bin/zk', ['f', 'env'], {
        cwd: process.env.ZKSYNC_HOME,
        env: {
            PATH: process.env.PATH,
            ZKSYNC_ENV: zksyncEnv,
            ZKSYNC_HOME: process.env.ZKSYNC_HOME
        }
    });
    return { ...process.env, ...dotenv.parse(res.stdout) };
}

/** Loads env profiles for the main and external nodes */
function loadEnvs() {
    let deploymentMode: string;
    if (fileConfig.loadFromFile) {
        const genesisConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'genesis.yaml' });
        deploymentMode = genesisConfig.deploymentMode;
    } else {
        deploymentMode = process.env.DEPLOYMENT_MODE ?? 'Rollup';
        if (!['Validium', 'Rollup'].includes(deploymentMode)) {
            throw new Error(`Unknown deployment mode: ${deploymentMode}`);
        }
    }
    console.log(`Using deployment mode: ${deploymentMode}`);

    let mainEnvName: string;
    let extEnvName: string;
    if (deploymentMode === 'Validium') {
        mainEnvName = process.env.IN_DOCKER ? 'dev_validium_docker' : 'dev_validium';
        extEnvName = process.env.IN_DOCKER ? 'ext-node-validium-docker' : 'ext-node-validium';
    } else {
        // Rollup deployment mode
        mainEnvName = process.env.IN_DOCKER ? 'docker' : 'dev';
        extEnvName = process.env.IN_DOCKER ? 'ext-node-docker' : 'ext-node';
    }

    console.log(`Fetching main node env: ${mainEnvName}`);
    const mainEnv = fetchEnv(mainEnvName);
    console.log(`Fetching EN env: ${extEnvName}`);
    const extEnv = fetchEnv(extEnvName);
    return [mainEnv, extEnv];
}

describe('Block reverting test', function () {
    let operatorAddress: string;
    let depositAmount: bigint;
    let mainNodeSpawner: NodeSpawner;
    let mainNode: Node<NodeType.MAIN>;
    let extNodeSpawner: NodeSpawner;
    let extNode: Node<NodeType.EXT>;
    let mainContract: IZkSyncHyperchain;
    let alice: zksync.Wallet;
    let depositL1BatchNumber: number;
    let batchesCommittedBeforeRevert: bigint;

    const autoKill: boolean = !fileConfig.loadFromFile || !process.env.NO_KILL;

    before('initialize test', async () => {
        let ethClientWeb3Url: string;
        let apiWeb3JsonRpcHttpUrl: string;
        let baseTokenAddress: string;
        let enEthClientUrl: string;

        const [mainEnv, extEnv] = loadEnvs();

        if (fileConfig.loadFromFile) {
            const secretsConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'secrets.yaml' });
            const generalConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'general.yaml' });
            const contractsConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'contracts.yaml' });
            const externalNodeGeneralConfig = loadConfig({
                pathToHome,
                configsFolderSuffix: 'external_node',
                chain: fileConfig.chain,
                config: 'general.yaml'
            });
            const walletsConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'wallets.yaml' });

            ethClientWeb3Url = secretsConfig.l1.l1_rpc_url;
            apiWeb3JsonRpcHttpUrl = generalConfig.api.web3_json_rpc.http_url;
            baseTokenAddress = contractsConfig.l1.base_token_addr;
            enEthClientUrl = externalNodeGeneralConfig.api.web3_json_rpc.http_url;
            operatorAddress = walletsConfig.operator.address;
        } else {
            ethClientWeb3Url = mainEnv.ETH_CLIENT_WEB3_URL!;
            apiWeb3JsonRpcHttpUrl = mainEnv.API_WEB3_JSON_RPC_HTTP_URL!;
            baseTokenAddress = mainEnv.CONTRACTS_BASE_TOKEN_ADDR!;
            enEthClientUrl = `http://127.0.0.1:${extEnv.EN_HTTP_PORT!}`;
            // TODO use env variable for this?
            operatorAddress = '0xde03a0B5963f75f1C8485B355fF6D30f3093BDE7';
        }

        const pathToMainLogs = await logsPath('server.log');
        const mainLogs = await fs.open(pathToMainLogs, 'a');
        console.log(`Writing main node logs to ${pathToMainLogs}`);

        const pathToEnLogs = await logsPath('external_node.log');
        const extLogs = await fs.open(pathToEnLogs, 'a');
        console.log(`Writing EN logs to ${pathToEnLogs}`);

        if (process.env.SKIP_COMPILATION !== 'true' && !fileConfig.loadFromFile) {
            compileBinaries();
        }
        const enableConsensus = process.env.ENABLE_CONSENSUS === 'true';

        console.log(`enableConsensus = ${enableConsensus}`);
        depositAmount = ethers.parseEther('0.001');

        const mainNodeSpawnOptions = {
            enableConsensus,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl,
            baseTokenAddress
        };
        mainNodeSpawner = new NodeSpawner(pathToHome, mainLogs, fileConfig, mainNodeSpawnOptions, mainEnv);
        const extNodeSpawnOptions = {
            enableConsensus,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl: enEthClientUrl,
            baseTokenAddress
        };
        extNodeSpawner = new NodeSpawner(pathToHome, extLogs, fileConfig, extNodeSpawnOptions, extEnv);
    });

    step('Make sure that nodes are not running', async () => {
        if (autoKill) {
            await Node.killAll(NodeType.MAIN);
            await Node.killAll(NodeType.EXT);
        }
    });

    step('Start main node', async () => {
        mainNode = await mainNodeSpawner.spawnMainNode(true);
    });

    step('Start external node', async () => {
        extNode = await extNodeSpawner.spawnExtNode();
    });

    step('Fund wallets', async () => {
        await mainNode.tester.fundSyncWallet();
        mainContract = await mainNode.tester.syncWallet.getMainContract();
        await extNode.tester.fundSyncWallet();
        alice = extNode.tester.emptyWallet();
    });

    step('Seal L1 batch', async () => {
        depositL1BatchNumber = await extNode.createBatchWithDeposit(alice.address, depositAmount);
    });

    step('wait for L1 batch to get executed', async () => {
        await waitToExecuteBatch(mainContract, depositL1BatchNumber);
    });

    step('Restart main node with batch execution turned off', async () => {
        await mainNode.killAndWaitForShutdown();
        mainNode = await mainNodeSpawner.spawnMainNode(false);
    });

    // FIXME: need 2 batches?
    step('seal another L1 batch', async () => {
        await extNode.createBatchWithDeposit(alice.address, depositAmount);
    });

    step('check wallet balance', async () => {
        const balance = await alice.getBalance();
        console.log(`Balance before revert: ${balance}`);
        assert(balance === depositAmount * 2n, 'Incorrect balance after deposits');
    });

    step('wait for the new batch to be committed', async () => {
        batchesCommittedBeforeRevert = await waitToCommitBatchesWithoutExecution(mainContract);
    });

    step('stop server', async () => {
        await mainNode.killAndWaitForShutdown();
    });

    step('revert batches', async () => {
        await executeRevert(pathToHome, fileConfig.chain, operatorAddress, batchesCommittedBeforeRevert, mainContract);
    });

    step('restart server', async () => {
        mainNode = await mainNodeSpawner.spawnMainNode(true);
    });

    step('Wait for EN to detect reorg and terminate', async () => {
        await extNode.waitForExit();
    });

    step('Restart EN', async () => {
        extNode = await extNodeSpawner.spawnExtNode();
    });

    step('wait until last deposit is re-executed', async () => {
        let balanceBefore;
        let tryCount = 0;
        while ((balanceBefore = await alice.getBalance()) !== 2n * depositAmount && tryCount < 30) {
            console.log(`Balance after revert: ${balanceBefore}`);
            tryCount++;
            await utils.sleep(1);
        }
        assert(balanceBefore === 2n * depositAmount, 'Incorrect balance after revert');
    });

    step('execute transaction after revert', async () => {
        await executeDepositAfterRevert(extNode.tester, alice, depositAmount);
        const balanceAfter = await alice.getBalance();
        console.log(`Balance after another deposit: ${balanceAfter}`);
        assert(balanceAfter === depositAmount * 3n, 'Incorrect balance after another deposit');
    });

    step('check random transfer', async () => {
        await checkRandomTransfer(alice, 1n);
    });

    after('terminate nodes', async () => {
        await mainNode.terminate();
        await extNode.terminate();

        if (fileConfig.loadFromFile) {
            replaceAggregatedBlockExecuteDeadline(pathToHome, fileConfig, 10);
        }
    });
});
