import { executeCommand, executeBackgroundCommand } from './execute-command';
import { FileMutex } from './file-mutex';
import * as console from "node:console";
import { ChildProcess } from 'child_process';
import { promisify } from "node:util";
import { exec } from "node:child_process";

/**
 * Global mutex for phase1 chain initialization (same as in create-chain.ts)
 */
const fileMutex = new FileMutex();

/**
 * External node handle interface
 */
export interface ExternalNodeHandle {
  chainName: string;
  process?: ChildProcess;
  kill(): Promise<void>;
}

async function killPidWithAllChilds(pid: number, signalNumber: number) {
  let childs = [pid];
  while (true) {
    try {
      let child = childs.at(-1);
      childs.push(+(await promisify(exec)(`pgrep -P ${child}`)).stdout);
    } catch (e) {
      break;
    }
  }
  // We always run the test using additional tools, that means we have to kill not the main process, but the child process
  for (let i = childs.length - 1; i >= 0; i--) {
    try {
      await promisify(exec)(`kill -${signalNumber} ${childs[i]}`);
    } catch (e) {}
  }
}

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
    await fileMutex.acquire();
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
      fileMutex.release();
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
export async function runExternalNode(chainName: string): Promise<ExternalNodeHandle> {
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
      case 'validium':
        runArgs.push('--components', 'all,da_fetcher');
        break;
      case 'da_migration':
        runArgs.push('--components', 'all,da_fetcher');
        break;
      case 'custom_token':
        // No additional arguments needed
        break;
      case 'era':
        // No additional arguments needed for era
        break;
      default:
        console.warn(`⚠️ Unknown chain type: ${chainType}, using default arguments`);
    }
    
    // Run the external node in background
    const process = await executeBackgroundCommand('zkstack', runArgs, chainName, "external_node");
    
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

    return {
      chainName,
      process,
      async kill() {
        if (process?.pid) {
          console.log(`🛑 Killing external node process for chain: ${chainName}`);
          await killPidWithAllChilds(process.pid, 9);
        } else {
          throw new Error("External node is not running!")
        }
      }
    };
  } catch (error) {
    console.error(`❌ Error running external node: ${error}`);
    throw error;
  }
} 
