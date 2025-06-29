import {describe, it} from 'vitest';
import {
    type ChainType,
    createChainAndStartServer,
    generateRealisticLoad,
    waitForAllBatchesToBeExecuted,
    initExternalNode,
    CHAIN_TYPES
} from '../src';
import {genesisRecoveryTest} from "../src/run-integration-tests";

describe('Genesis Recovery Test', () => {
    it.concurrent.each<ChainType>(CHAIN_TYPES)('for %s chain', async (chainType) => {
        const { chainName} = await createChainAndStartServer(chainType);

        await generateRealisticLoad(chainName);

        await waitForAllBatchesToBeExecuted(chainName);

        await initExternalNode(chainName)

        await genesisRecoveryTest(chainName);
    });
});
