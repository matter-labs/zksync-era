import {describe, it} from 'vitest';
import {
  type ChainType,
  createChainAndStartServer,
  generateRealisticLoad,
  waitForAllBatchesToBeExecuted,
  snapshotsRecoveryTest,
  TESTED_CHAIN_TYPE, initExternalNode
} from '../src';

describe('Snapshot Recovery Test', () => {
  it(`for ${TESTED_CHAIN_TYPE} chain`, async () => {
    const { chainName} = await createChainAndStartServer(TESTED_CHAIN_TYPE);

    await generateRealisticLoad(chainName);

    await waitForAllBatchesToBeExecuted(chainName);

    await initExternalNode(chainName);

    await snapshotsRecoveryTest(chainName);

  });
}); 
