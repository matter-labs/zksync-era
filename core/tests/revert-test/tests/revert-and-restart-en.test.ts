// Test of the behaviour of the external node when L1 batches get reverted.
//
// NOTE:
// main_contract.getTotalBatchesCommitted actually checks the number of batches committed.
// main_contract.getTotalBatchesExecuted actually checks the number of batches executed.
import * as utils from 'utils';
import { Tester } from './tester';
import {
    runExternalNodeInBackground,
    NodeInterface,
    MainNode,
    createBatchWithDeposit,
    waitToExecuteBatch,
    killServerAndWaitForShutdown,
    waitToCommitBatchesWithoutExecution,
    executeRevert,
    executeDepositAfterRevert,
    checkRandomTransfer,
    MainNodeSpawner
} from './utils';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { expect, assert } from 'chai';
import fs from 'node:fs/promises';
import * as child_process from 'child_process';
import * as dotenv from 'dotenv';
import { loadConfig, shouldLoadConfigFromFile, replaceAggregatedBlockExecuteDeadline } from 'utils/build/file-configs';
import path from 'path';
import { logsTestPath } from 'utils/build/logs';
import { killPidWithAllChilds } from 'utils/build/kill';
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

const [mainEnv, extEnv] = loadEnvs();

class ExtNode implements NodeInterface {
    private constructor(public readonly tester: Tester, private readonly proc: child_process.ChildProcess) {}

    public async terminate() {
        try {
            await killPidWithAllChilds(this.proc.pid!, 9);
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    // Terminates all main node processes running.
    //
    // WARNING: This is not safe to use when running nodes on multiple chains.
    public static async terminateAll() {
        try {
            await utils.exec('killall -INT zksync_external_node');
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    // Spawns an external node.
    // If enableConsensus is set, the node will use consensus P2P network to fetch blocks.
    public static async spawn(
        logs: fs.FileHandle,
        enableConsensus: boolean,
        ethClientWeb3Url: string,
        enEthClientUrl: string,
        baseTokenAddress: string
    ): Promise<ExtNode> {
        let args = []; // FIXME: unused
        if (enableConsensus) {
            args.push('--enable-consensus');
        }

        // Run server in background.
        let proc = runExternalNodeInBackground({
            stdio: ['ignore', logs, logs],
            cwd: pathToHome,
            env: extEnv,
            useZkInception: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });

        // Wait until the node starts responding.
        let tester: Tester = await Tester.init(ethClientWeb3Url, enEthClientUrl, baseTokenAddress);
        while (true) {
            try {
                await tester.syncWallet.provider.getBlockNumber();
                break;
            } catch (err) {
                if (proc.exitCode != null) {
                    assert.fail(`node failed to start, exitCode = ${proc.exitCode}`);
                }
                console.log(`ExtNode waiting for api endpoint on ${enEthClientUrl}`);
                await utils.sleep(1);
            }
        }
        return new ExtNode(tester, proc);
    }

    // Waits for the node process to exit.
    public async waitForExit(): Promise<number> {
        while (this.proc.exitCode === null) {
            await utils.sleep(1);
        }
        return this.proc.exitCode;
    }
}

describe('Block reverting test', function () {
    let ethClientWeb3Url: string;
    let apiWeb3JsonRpcHttpUrl: string;
    let baseTokenAddress: string;
    let enEthClientUrl: string;
    let operatorAddress: string;
    let mainLogs: fs.FileHandle;
    let extLogs: fs.FileHandle;
    let depositAmount: bigint;
    let enableConsensus: boolean;
    let mainNodeSpawner: MainNodeSpawner;
    let mainNode: MainNode;
    let extNode: ExtNode;
    let mainContract: IZkSyncHyperchain;
    let alice: zksync.Wallet;
    let initialL1BatchNumber: number;
    let batchesCommittedBeforeRevert: bigint;

    const autoKill: boolean = !fileConfig.loadFromFile || !process.env.NO_KILL;

    before('initialize test', async () => {
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
        mainLogs = await fs.open(pathToMainLogs, 'a');
        console.log(`Writing main node logs to ${pathToMainLogs}`);

        const pathToEnLogs = await logsPath('external_node.log');
        extLogs = await fs.open(pathToEnLogs, 'a');
        console.log(`Writing EN logs to ${pathToEnLogs}`);

        if (process.env.SKIP_COMPILATION !== 'true' && !fileConfig.loadFromFile) {
            compileBinaries();
        }
        enableConsensus = process.env.ENABLE_CONSENSUS === 'true';

        console.log(`enableConsensus = ${enableConsensus}`);
        depositAmount = ethers.parseEther('0.001');

        const mainNodeSpawnOptions = {
            enableConsensus,
            ethClientWeb3Url,
            apiWeb3JsonRpcHttpUrl,
            baseTokenAddress
        };
        mainNodeSpawner = new MainNodeSpawner(pathToHome, mainLogs, fileConfig, mainNodeSpawnOptions, mainEnv);
    });

    step('Make sure that nodes are not running', async () => {
        if (autoKill) {
            console.log('Make sure that nodes are not running');
            await ExtNode.terminateAll();
            await MainNode.terminateAll();
        }
    });

    step('Start main node', async () => {
        mainNode = await mainNodeSpawner.spawn(true);
    });

    step('Start external node', async () => {
        extNode = await ExtNode.spawn(extLogs, enableConsensus, ethClientWeb3Url, enEthClientUrl, baseTokenAddress);
    });

    step('Fund wallets', async () => {
        await mainNode.tester.fundSyncWallet();
        mainContract = await mainNode.tester.syncWallet.getMainContract();
        await extNode.tester.fundSyncWallet();
        alice = extNode.tester.emptyWallet();
    });

    step('seal L1 batch', async () => {
        initialL1BatchNumber = await createBatchWithDeposit(extNode, alice.address, depositAmount);
    });

    step('wait for L1 batch to get executed', async () => {
        await waitToExecuteBatch(mainContract, initialL1BatchNumber);
    });

    step('restart main node with batch execution turned off', async () => {
        await killServerAndWaitForShutdown(mainNode);
        mainNode = await mainNodeSpawner.spawn(false);
    });

    // FIXME: need 2 batches?
    step('seal another L1 batch', async () => {
        await createBatchWithDeposit(extNode, alice.address, depositAmount);
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
        await killServerAndWaitForShutdown(mainNode);
    });

    step('revert batches', async () => {
        await executeRevert(pathToHome, fileConfig.chain, operatorAddress, batchesCommittedBeforeRevert, mainContract);
    });

    step('restart server', async () => {
        mainNode = await mainNodeSpawner.spawn(true);
    });

    step('Wait for EN to detect reorg and terminate', async () => {
        await extNode.waitForExit();
    });

    step('Restart EN', async () => {
        extNode = await ExtNode.spawn(extLogs, enableConsensus, ethClientWeb3Url, enEthClientUrl, baseTokenAddress);
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
