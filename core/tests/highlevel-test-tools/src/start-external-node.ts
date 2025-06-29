import { executeCommand } from './execute-command';
import * as console from "node:console";

/**
 * Initializes an external node with the specified configuration
 * @param gatewayRpcUrl - Optional gateway RPC URL. If provided, will be used as --gateway-rpc-url parameter
 * @param chainName - The name of the chain (defaults to 'era')
 * @returns Promise that resolves when the external node is initialized
 */
export async function initExternalNode(chainName: string, gatewayRpcUrl?: string): Promise<void> {
  console.log(`🚀 Initializing external node for chain: ${chainName}`);
  
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
  
  try {
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
    ], chainName, '_external_node');
    
    console.log(`✅ External node initialized successfully: ${chainName}`);
  } catch (error) {
    console.error(`❌ Error during external node initialization: ${error}`);
    throw error;
  }
} 
