import { TestMaster } from '../../src/index';
import * as utils from 'zk/build/utils';
import fs from 'fs';
// import * as db from 'zk/build/database';
// import { clean } from 'zk/build/clean';
// import path from 'path';
// import * as env from 'zk/build/env';
// import { externalNode } from 'zk/build/server';

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
        await utils.spawn(`cd $ZKSYNC_HOME && cargo run --bin snapshot_creator --release`);
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

    test('snapshots can be created and contain valid values', async () => {
        let existingL1Batches = (await getAllSnapshots()).snapshots as any[];
        await runCreator();
        let newSnapshotsBatches = await getAllSnapshots();
        let addedSnapshots = (newSnapshotsBatches.snapshots as any[]).filter(
            (snapshot) => !existingL1Batches.find((other) => snapshot.l1BatchNumber === other.l1BatchNumber)
        );
        expect(addedSnapshots.length).toEqual(1);

        let l1BatchNumber = addedSnapshots[0].l1BatchNumber;
        let fullSnapshot = await getSnapshot(l1BatchNumber);
        let miniblockNumber = fullSnapshot.metadata.miniblockNumber;

        expect(fullSnapshot.metadata.l1BatchNumber).toEqual(addedSnapshots[0].l1BatchNumber);
        //TODO make this more generic so that it for instance works in GCS
        let path = `${process.env.ZKSYNC_HOME}/${process.env.OBJECT_STORE_FILE_BACKED_BASE_PATH}/storage_logs_snapshots/${fullSnapshot.storageLogsFiles[0]}`;

        let output = JSON.parse(fs.readFileSync(path).toString());

        for (const storageLog of output['storageLogs'] as any[]) {
            let snapshotAccountAddress = storageLog['key']['account']['address'];
            let snapshotKey = storageLog['key']['key'];
            let snapshotValue = storageLog['value'];
            let snapshotL1BatchNumber = storageLog['l1BatchNumber'];
            const valueOnBlockchain = await testMaster
                .mainAccount()
                .provider.getStorageAt(snapshotAccountAddress, snapshotKey, miniblockNumber);
            expect(snapshotValue).toEqual(valueOnBlockchain);
            expect(snapshotL1BatchNumber).toBeLessThanOrEqual(l1BatchNumber);
        }
    });

    // test('ext-node can be started using snapshot', async () => {
    //     process.chdir(process.env.ZKSYNC_HOME as string);
    //     let start_env = env.get();
    //     try {
    //         env.set('ext-node');
    //         await db.resetTest();
    //         await externalNode();
    //     } finally {
    //         env.set(start_env);
    //     }
    // });
});
