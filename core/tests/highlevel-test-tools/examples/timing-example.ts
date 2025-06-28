#!/usr/bin/env ts-node

import { createConsensusChain } from '../src';

async function demonstrateTiming() {
  console.log('🕐 Demonstrating chain creation timing utility...\n');
  
  try {
    const chainId = await createConsensusChain({
      l1RpcUrl: 'http://localhost:8545',
      serverDbUrl: 'postgres://postgres:notsecurepassword@localhost:5432',
      logsDir: './logs',
      cleanLogsOnStart: true,
      cleanChainsOnStart: true
    });
    
    console.log(`\n🎉 Successfully created chain: ${chainId}`);
    console.log('📁 Check the timing log file in ./logs/ for detailed timing information');
    
  } catch (error) {
    console.error('❌ Failed to create chain:', error);
    process.exit(1);
  }
}

// Run the example if this file is executed directly
if (require.main === module) {
  demonstrateTiming();
}

export { demonstrateTiming }; 