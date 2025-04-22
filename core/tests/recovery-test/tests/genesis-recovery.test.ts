import { expect } from 'chai';
import * as zksync from 'zksync-ethers';
import { ethers } from 'ethers';
import path from 'path';

import { NodeProcess, dropNodeData, getExternalNodeHealth, NodeComponents, sleep, FundedWallet } from '../src';
import { loadConfig } from 'utils/build/file-configs';
import { logsTestPath } from 'utils/build/logs';

const pathToHome = path.join(__dirname, '../../../..');

async function logsPath(chain: string, name: string): Promise<string> {
    return await logsTestPath(chain, 'logs/recovery/genesis', name);
}

/**
 * Tests recovery of an external node from scratch.
 *
 * Assumptions:
 *
 * - Main node is run for the duration of the test.
 * - "Rich wallet" 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 is funded on L1. This is always true if the environment
 *   was initialized via `zk init`.
 * - `ZKSYNC_ENV` variable is not set (checked at the start of the test). For this reason,
 *   the test doesn't have a `zk` wrapper; it should be launched using `yarn`.
 */
describe('genesis recovery', () => {
    /** Number of L1 batches for the node to process during each phase of the test. */
    const CATCH_UP_BATCH_COUNT = 3;

    const autoKill: boolean = !process.env.NO_KILL;
    const chainName = process.env.CHAIN_NAME!!;
    console.log(`Testing chain: ${chainName}`);

    let mainNode: zksync.Provider;
    let externalNode: zksync.Provider;
    let externalNodeProcess: NodeProcess;
    let externalNodeBatchNumber: number;

    let apiWeb3JsonRpcHttpUrl: string;
    let ethRpcUrl: string;
    let externalNodeUrl: string;
    let extNodeHealthUrl: string;

    before('prepare environment', async () => {
        const secretsConfig = loadConfig({ pathToHome, chain: chainName, config: 'secrets.yaml' });
        const generalConfig = loadConfig({ pathToHome, chain: chainName, config: 'general.yaml' });
        const externalNodeGeneralConfig = loadConfig({
            pathToHome,
            chain: chainName,
            configsFolderSuffix: 'external_node',
            config: 'general.yaml'
        });

        ethRpcUrl = secretsConfig.l1.l1_rpc_url;
        apiWeb3JsonRpcHttpUrl = generalConfig.api.web3_json_rpc.http_url;
        externalNodeUrl = externalNodeGeneralConfig.api.web3_json_rpc.http_url;
        extNodeHealthUrl = `http://127.0.0.1:${externalNodeGeneralConfig.api.healthcheck.port}/health`;

        mainNode = new zksync.Provider(apiWeb3JsonRpcHttpUrl);
        externalNode = new zksync.Provider(externalNodeUrl);
        if (autoKill) {
            await NodeProcess.stopAll('KILL');
        }
    });

    let fundedWallet: FundedWallet;

    before('create test wallet', async () => {
        console.log(`Using L1 RPC at ${ethRpcUrl}`);
        const eth = new ethers.JsonRpcProvider(ethRpcUrl);
        fundedWallet = await FundedWallet.create(mainNode, eth);
    });

    after(async () => {
        if (externalNodeProcess) {
            await externalNodeProcess.stopAndWait('KILL');
            await externalNodeProcess.logs.close();
        }
    });

    step('ensure that wallet has L2 funds', async () => {
        await fundedWallet.ensureIsFunded();
    });

    step('generate new batches if necessary', async () => {
        let pastL1BatchNumber = await mainNode.getL1BatchNumber();
        while (pastL1BatchNumber < CATCH_UP_BATCH_COUNT) {
            pastL1BatchNumber = await fundedWallet.generateL1Batch();
        }
    });

    step('drop external node data', async () => {
        await dropNodeData(chainName);
    });

    step('initialize external node w/o a tree', async () => {
        externalNodeProcess = await NodeProcess.spawn(
            await logsPath(chainName, 'external-node.log'),
            pathToHome,
            NodeComponents.WITH_TREE_FETCHER_AND_NO_TREE,
            chainName
        );

        const mainNodeBatchNumber = await mainNode.getL1BatchNumber();
        expect(mainNodeBatchNumber).to.be.greaterThanOrEqual(CATCH_UP_BATCH_COUNT);
        console.log(`Catching up to L1 batch #${CATCH_UP_BATCH_COUNT}`);

        let reorgDetectorSucceeded = false;
        let treeFetcherSucceeded = false;
        let consistencyCheckerSucceeded = false;

        while (!treeFetcherSucceeded || !reorgDetectorSucceeded || !consistencyCheckerSucceeded) {
            await sleep(1000);
            const health = await getExternalNodeHealth(extNodeHealthUrl);
            if (health === null) {
                // We do switch from l1 to gateway through restart for correctly handling it
                // we must restart the node manually
                if (externalNodeProcess.exitCode() != null) {
                    externalNodeProcess = await NodeProcess.spawn(
                        await logsPath(chainName, 'external-node.log'),
                        pathToHome,
                        NodeComponents.WITH_TREE_FETCHER_AND_NO_TREE,
                        chainName
                    );
                }
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

            if (!consistencyCheckerSucceeded) {
                const status = health.components.consistency_checker?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'ready']);
                const details = health.components.consistency_checker?.details;
                if (status === 'ready' && details !== undefined) {
                    console.log('Received consistency checker health details', details);
                    if (details.first_checked_batch !== undefined && details.last_checked_batch !== undefined) {
                        consistencyCheckerSucceeded = details.last_checked_batch >= CATCH_UP_BATCH_COUNT;
                    }
                }
            }
        }

        // If `externalNodeProcess` fails early, we'll trip these checks.
        expect(externalNodeProcess.exitCode()).to.be.null;
        expect(treeFetcherSucceeded, 'tree fetching failed').to.be.true;
        expect(reorgDetectorSucceeded, 'reorg detection check failed').to.be.true;
    });

    step('get EN batch number', async () => {
        externalNodeBatchNumber = await externalNode.getL1BatchNumber();
        console.log(`L1 batch number on EN: ${externalNodeBatchNumber}`);
        expect(externalNodeBatchNumber).to.be.greaterThanOrEqual(CATCH_UP_BATCH_COUNT);
    });

    step('stop EN', async () => {
        await externalNodeProcess.stopAndWait();
    });

    step('generate new batches for 2nd phase if necessary', async () => {
        let pastL1BatchNumber = await mainNode.getL1BatchNumber();
        while (pastL1BatchNumber < externalNodeBatchNumber + CATCH_UP_BATCH_COUNT) {
            pastL1BatchNumber = await fundedWallet.generateL1Batch();
        }
    });

    step('restart EN', async () => {
        externalNodeProcess = await NodeProcess.spawn(
            externalNodeProcess.logs,
            pathToHome,
            NodeComponents.WITH_TREE_FETCHER,
            chainName
        );

        let isNodeReady = false;
        while (!isNodeReady) {
            await sleep(1000);
            const health = await getExternalNodeHealth(extNodeHealthUrl);
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
        let consistencyCheckerSucceeded = false;

        while (!treeSucceeded || !reorgDetectorSucceeded || !consistencyCheckerSucceeded) {
            await sleep(1000);
            const health = await getExternalNodeHealth(extNodeHealthUrl);
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

            if (!consistencyCheckerSucceeded) {
                const status = health.components.consistency_checker?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'ready']);
                const details = health.components.consistency_checker?.details;
                if (status === 'ready' && details !== undefined) {
                    console.log('Received consistency checker health details', details);
                    if (details.first_checked_batch !== undefined && details.last_checked_batch !== undefined) {
                        consistencyCheckerSucceeded = details.last_checked_batch >= catchUpBatchNumber;
                    }
                }
            }
        }
    });
});
