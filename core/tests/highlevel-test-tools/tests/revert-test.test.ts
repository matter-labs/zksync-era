import {describe, it} from 'vitest';
import {
  type ChainType,
  createChainAndStartServer,
  generateRealisticLoad,
  waitForAllBatchesToBeExecuted,
  revertTest,
  ALL_CHAIN_TYPES, initExternalNode,
  runExternalNode, migrateToGatewayIfNeeded
} from '../src';

describe('Revert Test', () => {
  it.concurrent.each<ChainType>(ALL_CHAIN_TYPES)('for %s chain', async (chainType) => {
    const { chainName, serverHandle} = await createChainAndStartServer(chainType);

    await generateRealisticLoad(chainName);

    await waitForAllBatchesToBeExecuted(chainName);

    await initExternalNode(chainName);

    const externalNodeHandle = await runExternalNode(chainName);

    console.log(`😴 Sleeping for 60 seconds before killing external node to wait for it to sync..`);
    await new Promise(resolve => setTimeout(resolve, 60000));

    await externalNodeHandle.kill();

    await serverHandle.kill();

    await revertTest(chainName);

  });
}); 
