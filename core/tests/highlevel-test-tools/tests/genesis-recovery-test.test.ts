import {describe, it} from 'vitest';
import {
    type ChainType,
    createChainAndStartServer,
    generateRealisticLoad,
    waitForAllBatchesToBeExecuted,
    initExternalNode,
    TESTED_CHAIN_TYPE
} from '../src';
import {genesisRecoveryTest} from "../src/run-integration-tests";

describe('Genesis Recovery Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const { chainName} = await createChainAndStartServer(TESTED_CHAIN_TYPE);

        await generateRealisticLoad(chainName);

        await waitForAllBatchesToBeExecuted(chainName);

        await initExternalNode(chainName);

        await genesisRecoveryTest(chainName);
    });
});
