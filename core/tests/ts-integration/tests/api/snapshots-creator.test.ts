import { TestMaster } from '../../src/index';
import * as utils from 'zk/build/utils';
import fs from 'fs';
import * as zlib from 'zlib';
describe('Snapshots API tests', () => {
    let testMaster: TestMaster;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);

        if (process.env.ZKSYNC_ENV!.startsWith('ext-node')) {
            console.warn("You are trying to run snapshots creator tests on external node. It's not supported.");
        }
    });

    async function runCreator() {
        console.log('Starting creator');
        await utils.spawn(`cd $ZKSYNC_HOME && cargo run --bin snapshots_creator --release`);
    }

    async function rpcRequest(name: string, params: any) {
        const response = await testMaster.mainAccount().provider.send(name, params);
        console.log(response);
        return response;
    }

    async function getAllSnapshots() {
        return await rpcRequest('snapshots_getAllSnapshots', []);
    }

    async function getSnapshot(snapshotL1Batch: number) {
        return rpcRequest('snapshots_getSnapshot', [snapshotL1Batch]);
    }

    async function decompressGzip(filePath: string): Promise<string> {
        return new Promise((resolve, reject) => {
            const readStream = fs.createReadStream(filePath);
            const gunzip = zlib.createGunzip();
            let data = '';

            gunzip.on('data', (chunk) => (data += chunk.toString()));
            gunzip.on('end', () => resolve(data));
            gunzip.on('error', reject);

            readStream.pipe(gunzip);
        });
    }
    async function createAndValidateSnapshot() {
        let existingBatchNumbers = (await getAllSnapshots()).snapshotsL1BatchNumbers as number[];
        await runCreator();
        let newBatchNumbers = (await getAllSnapshots()).snapshotsL1BatchNumbers as number[];
        let addedSnapshots = newBatchNumbers.filter((x) => existingBatchNumbers.indexOf(x) === -1);
        expect(addedSnapshots.length).toEqual(1);

        let l1BatchNumber = addedSnapshots[0];
        let fullSnapshot = await getSnapshot(l1BatchNumber);
        let miniblockNumber = fullSnapshot.miniblockNumber;

        expect(fullSnapshot.l1BatchNumber).toEqual(l1BatchNumber);
        let path = `${process.env.ZKSYNC_HOME}/${fullSnapshot.storageLogsChunks[0].filepath}`;

        let output = JSON.parse(await decompressGzip(path));
        expect(output['storageLogs'].length > 0);

        for (const storageLog of output['storageLogs'] as any[]) {
            let snapshotAccountAddress = storageLog['key']['account']['address'];
            let snapshotKey = storageLog['key']['key'];
            let snapshotValue = storageLog['value'];
            let snapshotL1BatchNumber = storageLog['l1BatchNumberOfInitialWrite'];
            const valueOnBlockchain = await testMaster
                .mainAccount()
                .provider.getStorageAt(snapshotAccountAddress, snapshotKey, miniblockNumber);
            expect(snapshotValue).toEqual(valueOnBlockchain);
            expect(snapshotL1BatchNumber).toBeLessThanOrEqual(l1BatchNumber);
        }
    }

    test('snapshots can be created', async () => {
        await createAndValidateSnapshot();
    });
});
