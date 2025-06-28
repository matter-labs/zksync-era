import { runIntegrationTests } from './run-integration-tests';
import { executeCommand } from './execute-command';
import { getStepTimer } from './timing-tools';
import * as path from 'path';
import { getRpcUrl, queryJsonRpc, getL1BatchNumber } from './rpc-utils';

async function waitForL1Batch(chainName: string, timeoutMs: number = 300000): Promise<void> {
  console.log(`⏳ Waiting for L1 batch to be executed for chain: ${chainName}`);
  
  const startTime = Date.now();
  const maxWaitTime = timeoutMs;
  
  while (Date.now() - startTime < maxWaitTime) {
    try {
      const l1BatchNumber = await getL1BatchNumber(chainName);
      
      if (l1BatchNumber && l1BatchNumber > 0) {
        console.log(`✅ L1 batch executed successfully. Batch number: ${l1BatchNumber}`);
        return;
      }
      
      console.log(`⏳ Waiting for L1 batch... Current batch number: ${l1BatchNumber || 0}`);
      await new Promise(resolve => setTimeout(resolve, 5000));
    } catch (error) {
      console.log(`⚠️ Error checking L1 batch status, retrying... Error: ${error}`);
      await new Promise(resolve => setTimeout(resolve, 5000));
    }
  }
  
  throw new Error(`Timeout waiting for L1 batch execution after ${timeoutMs}ms`);
}

export async function generateLoad(chainName: string): Promise<void> {
  console.log(`🚀 Starting load generation for chain: ${chainName}`);
  
  const timer = getStepTimer(chainName);
  
  try {
    timer.startStep('ETH token checks integration test');
    await runIntegrationTests(chainName, 'ETH token checks');
    timer.endStep('ETH token checks integration test');
    
    timer.startStep('Wait for L1 batch execution');
    await waitForL1Batch(chainName);
    timer.endStep('Wait for L1 batch execution');
    
    timer.logTotalTime();
    console.log(`✅ Load generation completed successfully for chain: ${chainName}`);
  } catch (error) {
    console.error(`❌ Load generation failed for chain: ${chainName}`, error);
    throw error;
  }
} 