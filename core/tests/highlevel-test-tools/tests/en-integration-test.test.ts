import {describe, it} from 'vitest';
import {
    type ChainType,
    createChainAndStartServer,
    generateRealisticLoad,
    waitForAllBatchesToBeExecuted,
    initExternalNode,
    ALL_CHAIN_TYPES, runIntegrationTests, runExternalNode
} from '../src';
import {enIntegrationTests, genesisRecoveryTest} from "../src/run-integration-tests";

describe('Genesis Recovery Test', () => {
    it.concurrent.each<ChainType>(['validium'])('for %s chain', async (chainType) => {
        const { chainName} = await createChainAndStartServer(chainType);

        await generateRealisticLoad(chainName);

        await waitForAllBatchesToBeExecuted(chainName);

        await initExternalNode(chainName);

        await runExternalNode(chainName);

        await enIntegrationTests(chainName);
    });
});
