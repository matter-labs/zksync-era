import {describe, it} from 'vitest';
import {
  type ChainType,
  createChain,
  runIntegrationTests,
  waitForAllBatchesToBeExecuted,
  CHAIN_TYPES
} from '../src';
import {feesTest} from "../src/run-integration-tests";

describe('Fees Test', () => {
  it.concurrent.each<ChainType>(CHAIN_TYPES)('for %s chain', async (chainType) => {
    const { chainName, serverHandle } = await createChain(chainType);

    await runIntegrationTests(chainName, 'ETH token checks');

    await waitForAllBatchesToBeExecuted(chainName);

    await serverHandle.kill();

    await feesTest(chainName)

  });
});
