import {describe, it} from 'vitest';
import {
    type ChainType,
    createChainAndStartServer,
    generateRealisticLoad,
    waitForAllBatchesToBeExecuted,
    initExternalNode,
    ALL_CHAIN_TYPES, runExternalNode
} from '../src';
import {enIntegrationTests, genesisRecoveryTest} from "../src/run-integration-tests";

describe('External Node Integration tests Test', () => {
    it.concurrent.each<ChainType>(ALL_CHAIN_TYPES)('for %s chain', async (chainType) => {
        const { chainName} = await createChainAndStartServer(chainType);

        await generateRealisticLoad(chainName);

        await waitForAllBatchesToBeExecuted(chainName);

        await initExternalNode(chainName);

        await runExternalNode(chainName);

        await enIntegrationTests(chainName);
    });
});
