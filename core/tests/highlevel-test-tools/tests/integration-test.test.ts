import {describe, it} from "vitest";
import {CHAIN_TYPES, ChainType, createChain, runIntegrationTests} from "../src";

describe('Integration Test', () => {
    it.concurrent.each<ChainType>(CHAIN_TYPES)('for %s chain', async (chainType) => {
        const chain = await createChain(chainType);

        await runIntegrationTests(chain.chainName)

    }, 3600 * 1000);
});
