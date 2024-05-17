import { expect } from 'chai';
import fetch, { FetchError } from 'node-fetch';
import * as protobuf from 'protobufjs';
import * as zlib from 'zlib';
import fs, { FileHandle } from 'node:fs/promises';
import { ChildProcess, spawn, exec } from 'node:child_process';
import path from 'node:path';
import { promisify } from 'node:util';
import * as zksync from 'zksync-ethers';

interface AllSnapshotsResponse {
    readonly snapshotsL1BatchNumbers: number[];
}

interface GetSnapshotResponse {
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

interface Health<T> {
    readonly status: string;
    readonly details?: T;
}

interface SnapshotRecoveryDetails {
    readonly snapshot_l1_batch: number;
    readonly snapshot_l2_block: number;
    readonly factory_deps_recovered: boolean;
    readonly tokens_recovered: boolean;
    readonly storage_logs_chunks_left_to_process: number;
}

interface ConsistencyCheckerDetails {
    readonly first_checked_batch?: number;
    readonly last_checked_batch?: number;
}

interface ReorgDetectorDetails {
    readonly last_correct_l1_batch?: number;
    readonly last_correct_l2_block?: number;
}

interface TreeDetails {
    readonly min_l1_batch_number?: number | null;
}

interface DbPrunerDetails {
    readonly last_soft_pruned_l1_batch?: number;
    readonly last_hard_pruned_l1_batch?: number;
}

interface HealthCheckResponse {
    readonly components: {
        snapshot_recovery?: Health<SnapshotRecoveryDetails>;
        consistency_checker?: Health<ConsistencyCheckerDetails>;
        reorg_detector?: Health<ReorgDetectorDetails>;
        tree?: Health<TreeDetails>;
        db_pruner?: Health<DbPrunerDetails>;
        tree_pruner?: Health<{}>;
    };
}

/**
 * Tests snapshot recovery and node state pruning.
 *
 * Assumptions:
 *
 * - Main node is run for the duration of the test.
 * - `ZKSYNC_ENV` variable is not set (checked at the start of the test). For this reason,
 *   the test doesn't have a `zk` wrapper; it should be launched using `yarn`.
 */
describe('snapshot recovery', () => {
    const STORAGE_LOG_SAMPLE_PROBABILITY = 0.1;
    // Number of L1 batches awaited to be pruned.
    const PRUNED_BATCH_COUNT = 1;

    const homeDir = process.env.ZKSYNC_HOME!!;

    const externalNodeEnvProfile =
        'ext-node' +
        (process.env.DEPLOYMENT_MODE === 'Validium' ? '-validium' : '') +
        (process.env.IN_DOCKER ? '-docker' : '');
    console.log('Using external node env profile', externalNodeEnvProfile);
    let externalNodeEnv: { [key: string]: string } = {
        ...process.env,
        ZKSYNC_ENV: externalNodeEnvProfile,
        EN_SNAPSHOTS_RECOVERY_ENABLED: 'true'
    };

    let snapshotMetadata: GetSnapshotResponse;
    let mainNode: zksync.Provider;
    let externalNode: zksync.Provider;
    let externalNodeLogs: FileHandle;
    let externalNodeProcess: ChildProcess;

    let fundedWallet: zksync.Wallet;

    before('prepare environment', async () => {
        expect(process.env.ZKSYNC_ENV, '`ZKSYNC_ENV` should not be set to allow running both server and EN components')
            .to.be.undefined;
        mainNode = new zksync.Provider('http://127.0.0.1:3050');
        externalNode = new zksync.Provider('http://127.0.0.1:3060');
        await killExternalNode();
    });

    before('create test wallet', async () => {
        const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant/eth.json`);
        const ethTestConfig = JSON.parse(await fs.readFile(testConfigPath, { encoding: 'utf-8' }));
        const mnemonic = ethTestConfig.test_mnemonic as string;
        fundedWallet = zksync.Wallet.fromMnemonic(mnemonic, "m/44'/60'/0'/0/0").connect(mainNode);
    });

    after(async () => {
        if (externalNodeProcess) {
            externalNodeProcess.kill();
            await killExternalNode();
            await externalNodeLogs.close();
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
        const logs = await fs.open('snapshot-creator.log', 'w');
        const childProcess = spawn('zk run snapshots-creator', {
            cwd: homeDir,
            stdio: [null, logs.fd, logs.fd],
            shell: true
        });
        try {
            await waitForProcess(childProcess);
        } finally {
            childProcess.kill();
            await logs.close();
        }
    });

    step('validate snapshot', async () => {
        const allSnapshots = await getAllSnapshots();
        console.log('Obtained all snapshots', allSnapshots);
        const newBatchNumbers = allSnapshots.snapshotsL1BatchNumbers;

        const l1BatchNumber = Math.max(...newBatchNumbers);
        snapshotMetadata = await getSnapshot(l1BatchNumber);
        console.log('Obtained latest snapshot', snapshotMetadata);
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

                const snapshotAccountAddress = '0x' + storageLog.accountAddress.toString('hex');
                const snapshotKey = '0x' + storageLog.storageKey.toString('hex');
                const snapshotValue = '0x' + storageLog.storageValue.toString('hex');
                const snapshotL1BatchNumber = storageLog.l1BatchNumberOfInitialWrite;
                const valueOnBlockchain = await mainNode.getStorageAt(
                    snapshotAccountAddress,
                    snapshotKey,
                    l2BlockNumber
                );
                expect(snapshotValue).to.equal(valueOnBlockchain);
                expect(snapshotL1BatchNumber).to.be.lessThanOrEqual(l1BatchNumber);
            }
            console.log(`Checked random ${sampledCount} logs in the chunk`);
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

    step('initialize external node', async () => {
        externalNodeLogs = await fs.open('snapshot-recovery.log', 'w');
        externalNodeProcess = spawn('zk', externalNodeArgs(), {
            cwd: homeDir,
            stdio: [null, externalNodeLogs.fd, externalNodeLogs.fd],
            shell: true,
            env: externalNodeEnv
        });

        let recoveryFinished = false;
        let consistencyCheckerSucceeded = false;
        let reorgDetectorSucceeded = false;

        while (!recoveryFinished || !consistencyCheckerSucceeded || !reorgDetectorSucceeded) {
            await sleep(1000);
            const health = await getExternalNodeHealth();
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
        expect(externalNodeProcess.exitCode).to.be.null;
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
        await stopExternalNode();
        await waitForProcess(externalNodeProcess);

        const pruningParams = {
            EN_PRUNING_ENABLED: 'true',
            EN_PRUNING_REMOVAL_DELAY_SEC: '1',
            EN_PRUNING_DATA_RETENTION_SEC: '0', // immediately prune executed batches
            EN_PRUNING_CHUNK_SIZE: '1'
        };
        externalNodeEnv = { ...externalNodeEnv, ...pruningParams };
        console.log('Starting EN with pruning params', pruningParams);
        externalNodeProcess = spawn('zk', externalNodeArgs(), {
            cwd: homeDir,
            stdio: [null, externalNodeLogs.fd, externalNodeLogs.fd],
            shell: true,
            env: externalNodeEnv
        });

        let isDbPrunerReady = false;
        let isTreePrunerReady = false;
        while (!isDbPrunerReady || !isTreePrunerReady) {
            await sleep(1000);
            const health = await getExternalNodeHealth();
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
        }
    });

    // The logic below works fine if there is other transaction activity on the test network; we still
    // create *at least* `PRUNED_BATCH_COUNT + 1` L1 batches; thus, at least `PRUNED_BATCH_COUNT` of them
    // should be pruned eventually.
    step(`generate ${PRUNED_BATCH_COUNT + 1} transactions`, async () => {
        let pastL1BatchNumber = snapshotMetadata.l1BatchNumber;
        for (let i = 0; i < PRUNED_BATCH_COUNT + 1; i++) {
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

    step(`wait for pruning ${PRUNED_BATCH_COUNT} L1 batches`, async () => {
        const expectedPrunedBatchNumber = snapshotMetadata.l1BatchNumber + PRUNED_BATCH_COUNT;
        console.log(`Waiting for L1 batch #${expectedPrunedBatchNumber} to be pruned`);
        let isDbPruned = false;
        let isTreePruned = false;

        while (!isDbPruned || !isTreePruned) {
            await sleep(1000);
            const health = (await getExternalNodeHealth())!;

            const dbPrunerHealth = health.components.db_pruner!;
            console.log('DB pruner health', dbPrunerHealth);
            expect(dbPrunerHealth.status).to.be.equal('ready');
            isDbPruned = dbPrunerHealth.details!.last_hard_pruned_l1_batch! >= expectedPrunedBatchNumber;

            const treeHealth = health.components.tree!;
            console.log('Tree health', treeHealth);
            expect(treeHealth.status).to.be.equal('ready');
            const minTreeL1BatchNumber = treeHealth.details?.min_l1_batch_number;
            // The batch number pruned from the tree is one less than `minTreeL1BatchNumber`.
            isTreePruned = minTreeL1BatchNumber ? minTreeL1BatchNumber - 1 >= expectedPrunedBatchNumber : false;
        }
    });
});

async function waitForProcess(childProcess: ChildProcess) {
    await new Promise((resolve, reject) => {
        childProcess.on('error', (error) => {
            reject(error);
        });
        childProcess.on('exit', (code) => {
            if (code === 0) {
                resolve(undefined);
            } else {
                reject(new Error(`Process exited with non-zero code: ${code}`));
            }
        });
    });
}

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

async function sleep(millis: number) {
    await new Promise((resolve) => setTimeout(resolve, millis));
}

async function getExternalNodeHealth() {
    const EXTERNAL_NODE_HEALTH_URL = 'http://127.0.0.1:3081/health';

    try {
        const response: HealthCheckResponse = await fetch(EXTERNAL_NODE_HEALTH_URL).then((response) => response.json());
        return response;
    } catch (e) {
        let displayedError = e;
        if (e instanceof FetchError && e.code === 'ECONNREFUSED') {
            displayedError = '(connection refused)'; // Don't spam logs with "connection refused" messages
        }
        console.log(
            `Request to EN health check server failed ${displayedError}, in CI you can see more details 
            in "Show snapshot-creator.log logs" and "Show contract_verifier.log logs" steps`
        );
        return null;
    }
}

function externalNodeArgs() {
    const enableConsensus = process.env.ENABLE_CONSENSUS === 'true';
    const args = ['external-node', '--'];
    if (enableConsensus) {
        args.push('--enable-consensus');
    }
    return args;
}

async function stopExternalNode() {
    interface ChildProcessError extends Error {
        readonly code: number | null;
    }

    try {
        await promisify(exec)('killall -q -INT zksync_external_node');
    } catch (err) {
        const typedErr = err as ChildProcessError;
        if (typedErr.code === 1) {
            // No matching processes were found; this is fine.
        } else {
            throw err;
        }
    }
}

async function killExternalNode() {
    interface ChildProcessError extends Error {
        readonly code: number | null;
    }

    try {
        await promisify(exec)('killall -q -KILL zksync_external_node');
    } catch (err) {
        const typedErr = err as ChildProcessError;
        if (typedErr.code === 1) {
            // No matching processes were found; this is fine.
        } else {
            throw err;
        }
    }
}
