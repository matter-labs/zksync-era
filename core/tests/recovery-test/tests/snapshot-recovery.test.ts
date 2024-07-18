import { expect } from 'chai';
import * as protobuf from 'protobufjs';
import * as zlib from 'zlib';
import fs from 'node:fs/promises';
import path from 'node:path';
import { ethers } from 'ethers';
import * as zksync from 'zksync-ethers';

import {
    getExternalNodeHealth,
    sleep,
    NodeComponents,
    NodeProcess,
    dropNodeDatabase,
    dropNodeStorage,
    executeCommandWithLogs,
    FundedWallet
} from '../src';
import { loadConfig, shouldLoadConfigFromFile } from 'utils/build/file-configs';

const pathToHome = path.join(__dirname, '../../../..');
const fileConfig = shouldLoadConfigFromFile();

interface AllSnapshotsResponse {
    readonly snapshotsL1BatchNumbers: number[];
}

interface GetSnapshotResponse {
    readonly version: number;
    readonly miniblockNumber: number;
    readonly l1BatchNumber: number;
    readonly storageLogsChunks: Array<StorageLogChunkMetadata>;
}

interface StorageLogChunkMetadata {
    readonly filepath: string;
}

interface StorageLogChunk {
    readonly storageLogs: Array<StorageLog>;
}

interface StorageLog {
    readonly accountAddress: Buffer;
    readonly storageKey: Buffer;
    readonly storageValue: Buffer;
    readonly l1BatchNumberOfInitialWrite: number;
    readonly enumerationIndex: number;
}

interface TokenInfo {
    readonly l1_address: string;
    readonly l2_address: string;
}

/**
 * Tests snapshot recovery and node state pruning.
 *
 * Assumptions:
 *
 * - Main node is run for the duration of the test.
 * - "Rich wallet" 0x36615Cf349d7F6344891B1e7CA7C72883F5dc049 is funded on L1. This is always true if the environment
 *   was initialized via `zk init`.
 * - `ZKSYNC_ENV` variable is not set (checked at the start of the test). For this reason,
 *   the test doesn't have a `zk` wrapper; it should be launched using `yarn`.
 */
describe('snapshot recovery', () => {
    const STORAGE_LOG_SAMPLE_PROBABILITY = 0.1;
    // Number of L1 batches awaited to be pruned.
    const PRUNED_BATCH_COUNT = 1;

    const homeDir = process.env.ZKSYNC_HOME!!;

    const disableTreeDuringPruning = process.env.DISABLE_TREE_DURING_PRUNING === 'true';
    console.log(`Tree is ${disableTreeDuringPruning ? 'disabled' : 'enabled'} during pruning`);

    const externalNodeEnvProfile =
        'ext-node' +
        (process.env.DEPLOYMENT_MODE === 'Validium' ? '-validium' : '') +
        (process.env.IN_DOCKER ? '-docker' : '');
    console.log('Using external node env profile', externalNodeEnvProfile);
    let externalNodeEnv: { [key: string]: string } = {
        ...process.env,
        ZKSYNC_ENV: externalNodeEnvProfile,
        EN_SNAPSHOTS_RECOVERY_ENABLED: 'true',
        // Test parallel persistence for tree recovery, which is (yet) not enabled by default
        EN_EXPERIMENTAL_SNAPSHOTS_RECOVERY_TREE_PARALLEL_PERSISTENCE_BUFFER: '4'
    };

    let snapshotMetadata: GetSnapshotResponse;
    let mainNode: zksync.Provider;
    let externalNode: zksync.Provider;
    let externalNodeProcess: NodeProcess;

    let fundedWallet: FundedWallet;

    let apiWeb3JsonRpcHttpUrl: string;
    let ethRpcUrl: string;
    let externalNodeUrl: string;
    let extNodeHealthUrl: string;

    before('prepare environment', async () => {
        expect(process.env.ZKSYNC_ENV, '`ZKSYNC_ENV` should not be set to allow running both server and EN components')
            .to.be.undefined;

        if (fileConfig.loadFromFile) {
            const secretsConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'secrets.yaml' });
            const generalConfig = loadConfig({ pathToHome, chain: fileConfig.chain, config: 'general.yaml' });

            ethRpcUrl = secretsConfig.l1.l1_rpc_url;
            apiWeb3JsonRpcHttpUrl = generalConfig.api.web3_json_rpc.http_url;
            externalNodeUrl = 'http://127.0.0.1:3150';
            extNodeHealthUrl = 'http://127.0.0.1:3171/health';
        } else {
            apiWeb3JsonRpcHttpUrl = 'http://127.0.0.1:3050';
            ethRpcUrl = process.env.ETH_CLIENT_WEB3_URL ?? 'http://127.0.0.1:8545';
            externalNodeUrl = 'http://127.0.0.1:3060';
            extNodeHealthUrl = 'http://127.0.0.1:3081/health';
        }

        mainNode = new zksync.Provider(apiWeb3JsonRpcHttpUrl);
        externalNode = new zksync.Provider(externalNodeUrl);
        await NodeProcess.stopAll('KILL');
    });

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

    async function getAllSnapshots() {
        const output = await mainNode.send('snapshots_getAllSnapshots', []);
        return output as AllSnapshotsResponse;
    }

    async function getSnapshot(snapshotL1Batch: number) {
        const output = await mainNode.send('snapshots_getSnapshot', [snapshotL1Batch]);
        return output as GetSnapshotResponse;
    }

    async function getAllTokens(provider: zksync.Provider, atBlock?: number) {
        const output = await provider.send('en_syncTokens', atBlock ? [atBlock] : []);
        return output as TokenInfo[];
    }

    step('create snapshot', async () => {
        await executeCommandWithLogs('zk run snapshots-creator', 'snapshot-creator.log');
    });

    step('validate snapshot', async () => {
        const allSnapshots = await getAllSnapshots();
        console.log('Obtained all snapshots', allSnapshots);
        const newBatchNumbers = allSnapshots.snapshotsL1BatchNumbers;

        const l1BatchNumber = Math.max(...newBatchNumbers);
        snapshotMetadata = await getSnapshot(l1BatchNumber);
        console.log('Obtained latest snapshot', snapshotMetadata);
        expect(snapshotMetadata.version).to.be.oneOf([0, 1]);
        const l2BlockNumber = snapshotMetadata.miniblockNumber;

        const protoPath = path.join(homeDir, 'core/lib/types/src/proto/mod.proto');
        const root = await protobuf.load(protoPath);
        const SnapshotStorageLogsChunk = root.lookupType('zksync.types.SnapshotStorageLogsChunk');

        expect(snapshotMetadata.l1BatchNumber).to.equal(l1BatchNumber);
        for (const chunkMetadata of snapshotMetadata.storageLogsChunks) {
            const chunkPath = path.join(homeDir, chunkMetadata.filepath);
            console.log(`Checking storage logs chunk ${chunkPath}`);
            const output = SnapshotStorageLogsChunk.decode(await decompressGzip(chunkPath)) as any as StorageLogChunk;
            expect(output.storageLogs.length).to.be.greaterThan(0);
            console.log(`Decompressed chunk has ${output.storageLogs.length} logs`);

            let sampledCount = 0;
            for (const storageLog of output.storageLogs) {
                // Randomly sample logs to speed up the test.
                if (Math.random() > STORAGE_LOG_SAMPLE_PROBABILITY) {
                    continue;
                }
                sampledCount++;

                const snapshotL1BatchNumber = storageLog.l1BatchNumberOfInitialWrite;
                expect(snapshotL1BatchNumber).to.be.lessThanOrEqual(l1BatchNumber);

                if (snapshotMetadata.version === 0) {
                    const snapshotAccountAddress = '0x' + storageLog.accountAddress.toString('hex');
                    const snapshotKey = '0x' + storageLog.storageKey.toString('hex');
                    const snapshotValue = '0x' + storageLog.storageValue.toString('hex');
                    const valueOnBlockchain = await mainNode.getStorage(
                        snapshotAccountAddress,
                        snapshotKey,
                        l2BlockNumber
                    );
                    expect(snapshotValue).to.equal(valueOnBlockchain);
                }
            }
            console.log(`Checked random ${sampledCount} logs in the chunk`);
        }
    });

    step('drop external node database', async () => {
        await dropNodeDatabase(externalNodeEnv);
    });

    step('drop external node storage', async () => {
        await dropNodeStorage(externalNodeEnv);
    });

    step('initialize external node', async () => {
        externalNodeProcess = await NodeProcess.spawn(
            externalNodeEnv,
            'snapshot-recovery.log',
            pathToHome,
            fileConfig.loadFromFile
        );

        let recoveryFinished = false;
        let consistencyCheckerSucceeded = false;
        let reorgDetectorSucceeded = false;

        while (!recoveryFinished || !consistencyCheckerSucceeded || !reorgDetectorSucceeded) {
            await sleep(1000);
            const health = await getExternalNodeHealth(extNodeHealthUrl);
            if (health === null) {
                continue;
            }

            if (!recoveryFinished) {
                const status = health.components.snapshot_recovery?.status;
                expect(status).to.be.oneOf([undefined, 'affected', 'ready']);
                const details = health.components.snapshot_recovery?.details;
                if (details !== undefined) {
                    console.log('Received snapshot recovery health details', details);
                    expect(details.snapshot_l1_batch).to.equal(snapshotMetadata.l1BatchNumber);
                    expect(details.snapshot_l2_block).to.equal(snapshotMetadata.miniblockNumber);

                    if (
                        details.factory_deps_recovered &&
                        details.tokens_recovered &&
                        details.storage_logs_chunks_left_to_process === 0
                    ) {
                        console.log('Snapshot recovery is complete');
                        recoveryFinished = true;
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
                        expect(details.first_checked_batch).to.equal(snapshotMetadata.l1BatchNumber + 1);
                        expect(details.last_checked_batch).to.be.greaterThan(snapshotMetadata.l1BatchNumber);
                        consistencyCheckerSucceeded = true;
                    }
                }
            }

            if (!reorgDetectorSucceeded) {
                const status = health.components.reorg_detector?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'ready']);
                const details = health.components.reorg_detector?.details;
                if (status === 'ready' && details !== undefined) {
                    console.log('Received reorg detector health details', details);
                    if (details.last_correct_l1_batch !== undefined && details.last_correct_l2_block !== undefined) {
                        expect(details.last_correct_l1_batch).to.be.greaterThan(snapshotMetadata.l1BatchNumber);
                        expect(details.last_correct_l2_block).to.be.greaterThan(snapshotMetadata.miniblockNumber);
                        reorgDetectorSucceeded = true;
                    }
                }
            }
        }

        // If `externalNodeProcess` fails early, we'll trip these checks.
        expect(externalNodeProcess.exitCode()).to.be.null;
        expect(consistencyCheckerSucceeded, 'consistency check failed').to.be.true;
        expect(reorgDetectorSucceeded, 'reorg detection check failed').to.be.true;
    });

    step('check basic EN Web3 endpoints', async () => {
        const blockNumber = await externalNode.getBlockNumber();
        console.log(`Block number on EN: ${blockNumber}`);
        expect(blockNumber).to.be.greaterThan(snapshotMetadata.miniblockNumber);
        const l1BatchNumber = await externalNode.getL1BatchNumber();
        console.log(`L1 batch number on EN: ${l1BatchNumber}`);
        expect(l1BatchNumber).to.be.greaterThan(snapshotMetadata.l1BatchNumber);
    });

    step('check tokens on EN', async () => {
        const externalNodeTokens = await getAllTokens(externalNode);
        console.log('Fetched tokens from EN', externalNodeTokens);
        expect(externalNodeTokens).to.not.be.empty; // should contain at least ether
        const mainNodeTokens = await getAllTokens(mainNode, snapshotMetadata.miniblockNumber);
        console.log('Fetched tokens from main node', mainNodeTokens);

        // Sort tokens deterministically, since the order in which they are returned is not guaranteed.
        const compareFn = (x: TokenInfo, y: TokenInfo) => {
            expect(x.l2_address, 'Multiple tokens with same L2 address').to.not.equal(y.l2_address);
            return x.l2_address < y.l2_address ? -1 : 1;
        };
        externalNodeTokens.sort(compareFn);
        mainNodeTokens.sort(compareFn);

        expect(mainNodeTokens).to.deep.equal(externalNodeTokens);
    });

    step('restart EN', async () => {
        console.log('Stopping external node');
        await externalNodeProcess.stopAndWait();

        const components = disableTreeDuringPruning
            ? NodeComponents.WITH_TREE_FETCHER_AND_NO_TREE
            : NodeComponents.WITH_TREE_FETCHER;
        const pruningParams = {
            EN_PRUNING_ENABLED: 'true',
            EN_PRUNING_REMOVAL_DELAY_SEC: '1',
            EN_PRUNING_DATA_RETENTION_SEC: '0', // immediately prune executed batches
            EN_PRUNING_CHUNK_SIZE: '1'
        };
        externalNodeEnv = { ...externalNodeEnv, ...pruningParams };
        console.log('Starting EN with pruning params', pruningParams);
        externalNodeProcess = await NodeProcess.spawn(
            externalNodeEnv,
            externalNodeProcess.logs,
            pathToHome,
            fileConfig.loadFromFile,
            components
        );

        let isDbPrunerReady = false;
        let isTreePrunerReady = disableTreeDuringPruning; // skip health checks if we don't run the tree
        let isTreeFetcherReady = false;
        while (!isDbPrunerReady || !isTreePrunerReady || !isTreeFetcherReady) {
            await sleep(1000);
            const health = await getExternalNodeHealth(extNodeHealthUrl);
            if (health === null) {
                continue;
            }

            if (!isDbPrunerReady) {
                console.log('DB pruner health', health.components.db_pruner);
                const status = health.components.db_pruner?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'affected', 'ready']);
                isDbPrunerReady = status === 'ready';
            }
            if (!isTreePrunerReady) {
                console.log('Tree pruner health', health.components.tree_pruner);
                const status = health.components.tree_pruner?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'affected', 'ready']);
                isTreePrunerReady = status === 'ready';
            }
            if (!isTreeFetcherReady) {
                console.log('Tree fetcher health', health.components.tree_data_fetcher);
                const status = health.components.tree_data_fetcher?.status;
                expect(status).to.be.oneOf([undefined, 'not_ready', 'affected', 'ready']);
                isTreeFetcherReady = status === 'ready';
            }
        }
    });

    // The logic below works fine if there is other transaction activity on the test network; we still
    // create *at least* `PRUNED_BATCH_COUNT + 1` L1 batches; thus, at least `PRUNED_BATCH_COUNT` of them
    // should be pruned eventually.
    step(`generate ${PRUNED_BATCH_COUNT + 1} L1 batches`, async () => {
        for (let i = 0; i < PRUNED_BATCH_COUNT + 1; i++) {
            await fundedWallet.generateL1Batch();
        }
    });

    step(`wait for pruning ${PRUNED_BATCH_COUNT} L1 batches`, async () => {
        const expectedPrunedBatchNumber = snapshotMetadata.l1BatchNumber + PRUNED_BATCH_COUNT;
        console.log(`Waiting for L1 batch #${expectedPrunedBatchNumber} to be pruned`);
        let isDbPruned = false;
        let isTreePruned = disableTreeDuringPruning;

        while (!isDbPruned || !isTreePruned) {
            await sleep(1000);
            const health = (await getExternalNodeHealth(extNodeHealthUrl))!;

            const dbPrunerHealth = health.components.db_pruner!;
            console.log('DB pruner health', dbPrunerHealth);
            expect(dbPrunerHealth.status).to.be.equal('ready');
            isDbPruned = dbPrunerHealth.details!.last_hard_pruned_l1_batch! >= expectedPrunedBatchNumber;

            if (disableTreeDuringPruning) {
                expect(health.components.tree).to.be.undefined;
            } else {
                const treeHealth = health.components.tree!;
                console.log('Tree health', treeHealth);
                expect(treeHealth.status).to.be.equal('ready');
                const minTreeL1BatchNumber = treeHealth.details?.min_l1_batch_number;
                // The batch number pruned from the tree is one less than `minTreeL1BatchNumber`.
                isTreePruned = minTreeL1BatchNumber ? minTreeL1BatchNumber - 1 >= expectedPrunedBatchNumber : false;
            }
        }
    });
});

async function decompressGzip(filePath: string): Promise<Buffer> {
    const readStream = (await fs.open(filePath)).createReadStream();
    return new Promise((resolve, reject) => {
        const gunzip = zlib.createGunzip();
        let chunks: Uint8Array[] = [];

        gunzip.on('data', (chunk) => chunks.push(chunk));
        gunzip.on('end', () => resolve(Buffer.concat(chunks)));
        gunzip.on('error', reject);
        readStream.pipe(gunzip);
    });
}
