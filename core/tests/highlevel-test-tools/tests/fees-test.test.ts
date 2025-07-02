import { describe, it } from 'vitest';
import {
    type ChainType,
    createChainAndStartServer,
    generateRealisticLoad,
    waitForAllBatchesToBeExecuted,
    feesTest,
    TESTED_CHAIN_TYPE
} from '../src';

describe('Fees Test', () => {
    it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
        const { chainName, serverHandle } = await createChainAndStartServer(TESTED_CHAIN_TYPE);

        await generateRealisticLoad(chainName);

        await waitForAllBatchesToBeExecuted(chainName);

        await serverHandle.kill();

        await feesTest(chainName);
    });
});
