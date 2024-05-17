import { expect } from 'chai';
import * as zkweb3 from 'zksync-web3';
import fs, { FileHandle } from 'node:fs/promises';
import { ChildProcess, spawn } from 'node:child_process';
import path from 'node:path';
import * as zksync from 'zksync-ethers';

import {
    externalNodeArgs,
    getExternalNodeHealth,
    killExternalNode,
    NodeComponents,
    sleep,
    stopExternalNode,
    waitForProcess
} from '../src';

// FIXME: check consistency checker health once it has acceptable speed

/**
 * Tests recovery of an external node from scratch.
 *
 * Assumptions:
 *
 * - Main node is run for the duration of the test.
 * - "Rich wallet" 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 is funded on L2 (e.g., as a result of previous integration or load tests).
 * - `ZKSYNC_ENV` variable is not set (checked at the start of the test). For this reason,
 *   the test doesn't have a `zk` wrapper; it should be launched using `yarn`.
 */
describe('genesis recovery', () => {
    /** Number of L1 batches for the node to process during each phase of the test. */
    const CATCH_UP_BATCH_COUNT = 3;

    const homeDir = process.env.ZKSYNC_HOME!!;

    const externalNodeEnvProfile =
        'ext-node' +
        (process.env.DEPLOYMENT_MODE === 'Validium' ? '-validium' : '') +
        (process.env.IN_DOCKER ? '-docker' : '');
    console.log('Using external node env profile', externalNodeEnvProfile);
    let externalNodeEnv: { [key: string]: string } = {
        ...process.env,
        ZKSYNC_ENV: externalNodeEnvProfile,
        EN_SNAPSHOTS_RECOVERY_ENABLED: 'false'
    };

    let mainNode: zkweb3.Provider;
    let externalNode: zkweb3.Provider;
    let externalNodeLogs: FileHandle;
    let externalNodeProcess: ChildProcess;
    let externalNodeBatchNumber: number;

    before('prepare environment', async () => {
        expect(process.env.ZKSYNC_ENV, '`ZKSYNC_ENV` should not be set to allow running both server and EN components')
            .to.be.undefined;
        mainNode = new zkweb3.Provider('http://127.0.0.1:3050');
        externalNode = new zkweb3.Provider('http://127.0.0.1:3060');
        await killExternalNode();
    });

    let fundedWallet: zkweb3.Wallet;

    before('create test wallet', async () => {
        const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant/eth.json`);
        const ethTestConfig = JSON.parse(await fs.readFile(testConfigPath, { encoding: 'utf-8' }));
        const mnemonic = ethTestConfig.test_mnemonic as string;
        fundedWallet = zkweb3.Wallet.fromMnemonic(mnemonic, "m/44'/60'/0'/0/0").connect(mainNode);
    });

    after(async () => {
        if (externalNodeProcess) {
            externalNodeProcess.kill();
            await killExternalNode();
            await externalNodeLogs.close();
        }
    });

    step('generate new batches if necessary', async () => {
        let pastL1BatchNumber = await mainNode.getL1BatchNumber();
        while (pastL1BatchNumber < CATCH_UP_BATCH_COUNT) {
            const transactionResponse = await fundedWallet.transfer({
                to: fundedWallet.address,
                amount: 1,
                token: zksync.utils.ETH_ADDRESS
            });
            console.log('Generated a transaction from funded wallet', transactionResponse);
            const receipt = await transactionResponse.wait();
            console.log('Got finalized transaction receipt', receipt);

            // Wait until an L1 batch number with the transaction is sealed.
            let newL1BatchNumber: number;
            while ((newL1BatchNumber = await mainNode.getL1BatchNumber()) <= pastL1BatchNumber) {
                await sleep(1000);
            }
            console.log(`Sealed L1 batch #${newL1BatchNumber}`);
            pastL1BatchNumber = newL1BatchNumber;
        }
    });

    step('drop external node database', async () => {
        const childProcess = spawn('zk db reset', {
            cwd: homeDir,
            stdio: 'inherit',
            shell: true,
            env: externalNodeEnv
        });
        try {
            await waitForProcess(childProcess);
        } finally {
            childProcess.kill();
        }
    });

    step('drop external node storage', async () => {
        const childProcess = spawn('zk clean --database', {
            cwd: homeDir,
            stdio: 'inherit',
            shell: true,
            env: externalNodeEnv
        });
        try {
            await waitForProcess(childProcess);
        } finally {
            childProcess.kill();
        }
    });

    step('initialize external node w/o a tree', async () => {
        externalNodeLogs = await fs.open('genesis-recovery.log', 'w');
        externalNodeProcess = spawn('zk', externalNodeArgs(NodeComponents.WITH_TREE_FETCHER_AND_NO_TREE), {
            cwd: homeDir,
            stdio: [null, externalNodeLogs.fd, externalNodeLogs.fd],
            shell: true,
            env: externalNodeEnv
        });

        const mainNodeBatchNumber = await mainNode.getL1BatchNumber();
        expect(mainNodeBatchNumber).to.be.greaterThanOrEqual(CATCH_UP_BATCH_COUNT);
        console.log(`Catching up to L1 batch #${CATCH_UP_BATCH_COUNT}`);

        let reorgDetectorSucceeded = false;
        let treeFetcherSucceeded = false;

        while (!treeFetcherSucceeded || !reorgDetectorSucceeded) {
            await sleep(1000);
            const health = await getExternalNodeHealth();
            if (health === null) {
                continue;
            }

            if (!treeFetcherSucceeded) {
                const status = health.components.tree_data_fetcher?.status;
                const details = health.components.tree_data_fetcher?.details;
                if (status === 'ready' && details !== undefined && details.last_updated_l1_batch !== undefined) {
                    console.log('Received tree health details', details);
                    treeFetcherSucceeded = details.last_updated_l1_batch >= CATCH_UP_BATCH_COUNT;
                }
            }

            if (!reorgDetectorSucceeded) {
                const status = health.components.reorg_detector?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'ready']);
                const details = health.components.reorg_detector?.details;
                if (status === 'ready' && details !== undefined) {
                    console.log('Received reorg detector health details', details);
                    if (details.last_correct_l1_batch !== undefined) {
                        reorgDetectorSucceeded = details.last_correct_l1_batch >= CATCH_UP_BATCH_COUNT;
                    }
                }
            }
        }

        // If `externalNodeProcess` fails early, we'll trip these checks.
        expect(externalNodeProcess.exitCode).to.be.null;
        expect(treeFetcherSucceeded, 'tree fetching failed').to.be.true;
        expect(reorgDetectorSucceeded, 'reorg detection check failed').to.be.true;
    });

    step('get EN batch number', async () => {
        externalNodeBatchNumber = await externalNode.getL1BatchNumber();
        console.log(`L1 batch number on EN: ${externalNodeBatchNumber}`);
        expect(externalNodeBatchNumber).to.be.greaterThanOrEqual(CATCH_UP_BATCH_COUNT);
    });

    step('stop EN', async () => {
        await stopExternalNode();
        await waitForProcess(externalNodeProcess);
    });

    step('generate new batches for 2nd phase if necessary', async () => {
        let pastL1BatchNumber = await mainNode.getL1BatchNumber();
        while (pastL1BatchNumber < externalNodeBatchNumber + CATCH_UP_BATCH_COUNT) {
            const transactionResponse = await fundedWallet.transfer({
                to: fundedWallet.address,
                amount: 1,
                token: zksync.utils.ETH_ADDRESS
            });
            console.log('Generated a transaction from funded wallet', transactionResponse);
            const receipt = await transactionResponse.wait();
            console.log('Got finalized transaction receipt', receipt);

            // Wait until an L1 batch number with the transaction is sealed.
            let newL1BatchNumber: number;
            while ((newL1BatchNumber = await mainNode.getL1BatchNumber()) <= pastL1BatchNumber) {
                await sleep(1000);
            }
            console.log(`Sealed L1 batch #${newL1BatchNumber}`);
            pastL1BatchNumber = newL1BatchNumber;
        }
    });

    step('restart EN', async () => {
        externalNodeProcess = spawn('zk', externalNodeArgs(NodeComponents.WITH_TREE_FETCHER), {
            cwd: homeDir,
            stdio: [null, externalNodeLogs.fd, externalNodeLogs.fd],
            shell: true,
            env: externalNodeEnv
        });

        let isNodeReady = false;
        while (!isNodeReady) {
            await sleep(1000);
            const health = await getExternalNodeHealth();
            if (health === null) {
                continue;
            }
            console.log('Node health', health);
            isNodeReady = health.status === 'ready';
        }
    });

    step('wait for tree to catch up', async () => {
        const mainNodeBatchNumber = await mainNode.getL1BatchNumber();
        expect(mainNodeBatchNumber).to.be.greaterThanOrEqual(externalNodeBatchNumber + CATCH_UP_BATCH_COUNT);
        const catchUpBatchNumber = Math.min(mainNodeBatchNumber, externalNodeBatchNumber + CATCH_UP_BATCH_COUNT);
        console.log(`Catching up to L1 batch #${catchUpBatchNumber}`);

        let reorgDetectorSucceeded = false;
        let treeSucceeded = false;

        while (!treeSucceeded || !reorgDetectorSucceeded) {
            await sleep(1000);
            const health = await getExternalNodeHealth();
            if (health === null) {
                continue;
            }

            if (!treeSucceeded) {
                const status = health.components.tree?.status;
                const details = health.components.tree?.details;
                if (status === 'ready' && details !== undefined && details.next_l1_batch_number !== undefined) {
                    console.log('Received tree health details', details);
                    expect(details.min_l1_batch_number).to.be.equal(0);
                    treeSucceeded = details.next_l1_batch_number > catchUpBatchNumber;
                }
            }

            if (!reorgDetectorSucceeded) {
                const status = health.components.reorg_detector?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'ready']);
                const details = health.components.reorg_detector?.details;
                if (status === 'ready' && details !== undefined) {
                    console.log('Received reorg detector health details', details);
                    if (details.last_correct_l1_batch !== undefined) {
                        reorgDetectorSucceeded = details.last_correct_l1_batch >= catchUpBatchNumber;
                    }
                }
            }
        }
    });
});
