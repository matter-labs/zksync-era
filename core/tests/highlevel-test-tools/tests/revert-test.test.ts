import { describe, it } from 'vitest';
import {
    createChainAndStartServer,
    generateRealisticLoad,
    waitForAllBatchesToBeExecuted,
    revertTest,
    TESTED_CHAIN_TYPE,
    initExternalNode,
    runExternalNode
} from '../src';

describe('Revert Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const { chainName, serverHandle } = await createChainAndStartServer(TESTED_CHAIN_TYPE);

        await generateRealisticLoad(chainName);

        await waitForAllBatchesToBeExecuted(chainName);

        await initExternalNode(chainName);

        const externalNodeHandle = await runExternalNode(chainName);

        console.log(`😴 Sleeping for 60 seconds before killing external node to wait for it to sync..`);
        await new Promise((resolve) => setTimeout(resolve, 60000));

        await externalNodeHandle.kill();

        await serverHandle.kill();

        await revertTest(chainName);
    });
});
