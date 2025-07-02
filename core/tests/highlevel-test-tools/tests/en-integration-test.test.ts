import { describe, it } from 'vitest';
import {
    type ChainType,
    createChainAndStartServer,
    generateRealisticLoad,
    waitForAllBatchesToBeExecuted,
    initExternalNode,
    runExternalNode,
    TESTED_CHAIN_TYPE
} from '../src';
import { enIntegrationTests, genesisRecoveryTest } from '../src/run-integration-tests';

describe('External Node Integration tests Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const { chainName } = await createChainAndStartServer(TESTED_CHAIN_TYPE);

        await generateRealisticLoad(chainName);

        await waitForAllBatchesToBeExecuted(chainName);

        await initExternalNode(chainName);

        await runExternalNode(chainName);

        await enIntegrationTests(chainName);
    });
});
