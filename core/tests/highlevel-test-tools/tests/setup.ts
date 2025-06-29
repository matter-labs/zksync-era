// Test setup file for Vitest
import { beforeAll, afterAll } from 'vitest';
import { exec } from 'child_process';
import { promisify } from 'util';
import { 
  cleanHistoricalLogs, 
  cleanMutexLockFiles, 
  cleanTestChains 
} from '../src';
import { setupDbTemplate } from '../src/create-chain';

const execAsync = promisify(exec);

// Global test setup
beforeAll(async () => {
  console.log('Setting up test environment...');
  
  try {
    // Kill any existing zksync_server processes
    await execAsync('pkill zksync_server');
  } catch (error) {
    // Ignore errors if no processes were found
  }

  // Clean historical logs at the start of integration tests
  cleanHistoricalLogs();
  
  // Clean test chains
  cleanTestChains('../../.././chains');
  
  // Clean mutex lock files
  cleanMutexLockFiles();
  
  // Setup database template
  await setupDbTemplate();
});

// Global test cleanup
afterAll(() => {
  console.log('Cleaning up test environment...');
}); 
