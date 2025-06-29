import { executeCommand, executeBackgroundCommand } from './execute-command';
import { FileMutex } from './file-mutex';
import * as console from "node:console";

/**
 * Global mutex for phase1 chain initialization (same as in create-chain.ts)
 */
const phase1Mutex = new FileMutex('zkstack_chain_init_phase1');

/**
 * Initializes an external node with the specified configuration
 * @param gatewayRpcUrl - Optional gateway RPC URL. If provided, will be used as --gateway-rpc-url parameter
 * @param chainName - The name of the chain (defaults to 'era')
 * @returns Promise that resolves when the external node is initialized
 */
export async function initExternalNode(chainName: string = 'era', gatewayRpcUrl?: string): Promise<void> {
  console.log(`🚀 Initializing external node for chain: ${chainName}`);
  
  try {
    // Acquire mutex for external node initialization
    console.log(`🔒 Acquiring mutex for external node initialization of ${chainName}...`);
    await phase1Mutex.acquire();
    console.log(`✅ Mutex acquired for external node initialization of ${chainName}`);
    
    try {
      // Build the configs command arguments
      const configsArgs = [
        'external-node', 'configs',
        '--db-url=postgres://postgres:notsecurepassword@localhost:5432',
        `--db-name=${chainName}_external_node`,
        '--l1-rpc-url=http://localhost:8545',
        '--chain', chainName,
        '--tight-ports'
      ];
      
      // Add gateway RPC URL if provided
      if (gatewayRpcUrl) {
        configsArgs.push(`--gateway-rpc-url=${gatewayRpcUrl}`);
      }
      
      console.log(`⏳ Configuring external node: ${chainName}`);
      
      // Execute the configs command
      await executeCommand('zkstack', configsArgs, chainName, 'external_node');
      
      console.log(`✅ External node configured successfully: ${chainName}`);
      
      console.log(`⏳ Initializing external node: ${chainName}`);
      
      // Execute the init command
      await executeCommand('zkstack', [
        'external-node', 'init',
        '--ignore-prerequisites',
        '--chain', chainName
      ], chainName, 'external_node');
      
      console.log(`✅ External node initialized successfully: ${chainName}`);
    } finally {
      phase1Mutex.release();
    }
  } catch (error) {
    console.error(`❌ Error during external node initialization: ${error}`);
    throw error;
  }
}

/**
 * Runs an external node with the appropriate configuration based on chain type
 * @param chainName - The name of the chain (chain type will be extracted by removing last 9 characters)
 * @returns Promise that resolves when the external node is running
 */
export async function runExternalNode(chainName: string): Promise<void> {
  // Extract chain type by removing last 9 characters (UUID suffix)
  const chainType = chainName.slice(0, -9);
  
  console.log(`🚀 Running external node for chain: ${chainName} (type: ${chainType})`);
  
  try {
    // Step 1: Run external node with chain-specific arguments
    console.log(`⏳ Starting external node: ${chainName}`);
    
    const runArgs = [
      'external-node', 'run',
      '--ignore-prerequisites',
      '--chain', chainName
    ];
    
    // Add chain-specific arguments
    switch (chainType) {
      case 'consensus':
        runArgs.push('--enable-consensus');
        break;
      case 'validium':
        runArgs.push('--components', 'all,da_fetcher');
        break;
      case 'da_migration':
        runArgs.push('--components', 'all,da_fetcher');
        break;
      case 'custom_token':
        // No additional arguments needed
        break;
      default:
        console.warn(`⚠️ Unknown chain type: ${chainType}, using default arguments`);
    }
    
    // Run the external node in background
    await executeBackgroundCommand('zkstack', runArgs, chainName, "_external_node");
    
    console.log(`✅ External node started successfully: ${chainName}`);
    
    // Step 2: Wait for external node to be ready
    console.log(`⏳ Waiting for external node to be ready: ${chainName}`);
    await executeCommand('zkstack', [
      'external-node', 'wait',
      '--ignore-prerequisites',
      '--verbose',
      '--chain', chainName
    ], chainName, 'external_node');
    
    console.log(`✅ External node is ready: ${chainName}`);
  } catch (error) {
    console.error(`❌ Error running external node: ${error}`);
    throw error;
  }
} 
