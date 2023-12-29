import { TestMaster } from '../../src/index';
import fs from 'fs';
import * as zlib from 'zlib';
import * as protobuf from 'protobufjs';
import { snapshots_creator } from 'zk/build/run/run';
import path from 'path';

describe('Snapshots API tests', () => {
    let testMaster: TestMaster;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);

        if (process.env.ZKSYNC_ENV!.startsWith('ext-node')) {
            console.warn("You are trying to run snapshots creator tests on external node. It's not supported.");
        }
    });

    async function runCreator() {
        await snapshots_creator();
    }

    async function rpcRequest(name: string, params: any) {
        const response = await testMaster.mainAccount().provider.send(name, params);
        return response;
    }

    async function getAllSnapshots() {
        return await rpcRequest('snapshots_getAllSnapshots', []);
    }

    async function getSnapshot(snapshotL1Batch: number) {
        return rpcRequest('snapshots_getSnapshot', [snapshotL1Batch]);
    }

    async function decompressGzip(filePath: string): Promise<Buffer> {
        return new Promise((resolve, reject) => {
            const readStream = fs.createReadStream(filePath);
            const gunzip = zlib.createGunzip();
            let chunks: Uint8Array[] = [];

            gunzip.on('data', (chunk) => chunks.push(chunk));
            gunzip.on('end', () => resolve(Buffer.concat(chunks)));
            gunzip.on('error', reject);

            readStream.pipe(gunzip);
        });
    }
    async function createAndValidateSnapshot() {
        const existingBatchNumbers = (await getAllSnapshots()).snapshotsL1BatchNumbers as number[];
        await runCreator();
        const newBatchNumbers = (await getAllSnapshots()).snapshotsL1BatchNumbers as number[];
        const addedSnapshots = newBatchNumbers.filter((x) => existingBatchNumbers.indexOf(x) === -1);
        expect(addedSnapshots.length).toEqual(1);

        const l1BatchNumber = addedSnapshots[0];
        const fullSnapshot = await getSnapshot(l1BatchNumber);
        const miniblockNumber = fullSnapshot.miniblockNumber;

        const protoPath = path.join(process.env.ZKSYNC_HOME as string, 'core/lib/types/src/proto/mod.proto');
        const root = await protobuf.load(protoPath);
        const SnapshotStorageLogsChunk = root.lookupType('zksync.types.SnapshotStorageLogsChunk');

        expect(fullSnapshot.l1BatchNumber).toEqual(l1BatchNumber);
        for (let chunkMetadata of fullSnapshot.storageLogsChunks) {
            const chunkPath = path.join(process.env.ZKSYNC_HOME as string, chunkMetadata.filepath);
            const output = SnapshotStorageLogsChunk.decode(await decompressGzip(chunkPath)) as any;
            expect(output['storageLogs'].length > 0);
            for (const storageLog of output['storageLogs'] as any[]) {
                const snapshotAccountAddress = '0x' + storageLog['accountAddress'].toString('hex');
                const snapshotKey = '0x' + storageLog['storageKey'].toString('hex');
                const snapshotValue = '0x' + storageLog['storageValue'].toString('hex');
                const snapshotL1BatchNumber = storageLog['l1BatchNumberOfInitialWrite'];
                const valueOnBlockchain = await testMaster
                    .mainAccount()
                    .provider.getStorageAt(snapshotAccountAddress, snapshotKey, miniblockNumber);
                expect(snapshotValue).toEqual(valueOnBlockchain);
                expect(snapshotL1BatchNumber).toBeLessThanOrEqual(l1BatchNumber);
            }
        }
    }

    test('snapshots can be created', async () => {
        await createAndValidateSnapshot();
    });
});
