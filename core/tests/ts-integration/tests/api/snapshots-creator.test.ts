import fetch from 'node-fetch';
import { TestMaster } from '../../src/index';
import * as utils from 'zk/build/utils';
import fs from 'fs';

describe('Snapshots API tests', () => {
    let testMaster: TestMaster;

    beforeAll(() => {
        testMaster = TestMaster.getInstance(__filename);

        if (process.env.ZKSYNC_ENV!.startsWith('ext-node')) {
            console.warn("You are trying to run contract verification tests on external node. It's not supported.");
        }
    });

    async function runCreator() {
        console.log('Starting creator');
        await utils.spawn(`cd $ZKSYNC_HOME && cargo run --bin snapshot_creator --release`);
    }
    async function getRequest(url: string) {
        const init = {
            method: 'GET',
            headers: {
                'Content-Type': 'application/json'
            }
        };
        return await (await fetch(new URL(url, testMaster.environment().snapshotsUrl), init)).json();
    }

    async function getAllSnapshots() {
        return getRequest('/snapshots');
    }

    async function getSnapshot(snapshotL1Batch: number) {
        return getRequest(`/snapshots/${snapshotL1Batch}`);
    }

    test('most basic API test', async () => {
        let existingL1Batches = (await getAllSnapshots()).snapshots as any[];
        await runCreator();
        let newSnapshotsBatches = await getAllSnapshots();
        let addedSnapshots = (newSnapshotsBatches.snapshots as any[]).filter(
            (snapshot) => !existingL1Batches.find((other) => snapshot.l1BatchNumber === other.l1BatchNumber)
        );
        expect(addedSnapshots.length).toEqual(1);

        let expectedBatchNumber = addedSnapshots[0].l1BatchNumber;
        let fullSnapshot = await getSnapshot(expectedBatchNumber);

        expect(fullSnapshot.l1BatchNumber).toEqual(addedSnapshots[0].l1BatchNumber);
        //TODO make this more generic so that it for instance works in GCS
        let path = `${process.env.ZKSYNC_HOME}/${process.env.OBJECT_STORE_FILE_BACKED_BASE_PATH}/storage_logs_snapshots/${fullSnapshot.storageLogsFiles[0]}`;

        let output = JSON.parse(fs.readFileSync(path).toString());
        expect(output['l1_batch_number']).toEqual(expectedBatchNumber);
    });
});
