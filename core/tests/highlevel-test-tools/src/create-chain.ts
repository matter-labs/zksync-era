import { spawn } from 'child_process';
import { promisify } from 'util';
import * as fs from 'fs';
import * as path from 'path';
import { v4 as uuidv4 } from 'uuid';
import { executeCommand } from './execute-command';
import { FileMutex, cleanTestChains } from './file-mutex';
import { startServer, ServerHandle } from './start-server';

export type ChainType = 'consensus' | 'validium' | 'da_migration' | 'custom_token' | 'era';

export interface ChainConfig {
  l1RpcUrl?: string;
  serverDbUrl?: string;
}

/**
 * Global mutex for phase1 chain initialization
 */
const fileMutex = new FileMutex();

/**
 * Reads the custom token address from the erc20.yaml configuration file
 */
export function getCustomTokenAddress(configPath: string = '../../../configs/erc20.yaml'): string {
  try {
    if (!fs.existsSync(configPath)) {
      throw new Error(`Config file ${configPath} not found`);
    }

    const fileContent = fs.readFileSync(configPath, 'utf8');
    
    // Parse YAML as string and extract DAI token address using regex
    const daiAddressMatch = fileContent.match(/DAI:\s*\n\s*address:\s*(0x[a-fA-F0-9]{40})/);
    
    if (daiAddressMatch && daiAddressMatch[1]) {
      const tokenAddress = daiAddressMatch[1];
      console.log(`✅ Found custom token address: ${tokenAddress}`);
      return tokenAddress;
    } else {
      throw new Error(`Custom token address not found in config file ${configPath}`);
    }
  } catch (error) {
    console.error(`❌ Error reading custom token address from ${configPath}:`, error);
    throw error;
  }
}

/**
 * Creates and initializes a chain based on the specified chain type.
 * Supports four predefined chain types: consensus, validium, da_migration, custom_token
 * Returns the chain ID, configuration, and server handle
 */
export async function createChainAndStartServer(chainType: ChainType): Promise<{ chainName: string; serverHandle: ServerHandle }> {
  
  // Default configuration
  const finalConfig: ChainConfig = {
    l1RpcUrl: 'http://localhost:8545',
    serverDbUrl: 'postgres://postgres:notsecurepassword@localhost:5432',
  };
  
  // Generate UUID for unique chain name
  const uuid = uuidv4().replace(/-/g, '').substring(0, 8);
  
  // Generate random chain ID (between 1000 and 999999 to avoid conflicts)
  const randomChainId = Math.floor(Math.random() * 999000) + 1000;
  
  const ethTokenAddress = '0x0000000000000000000000000000000000000001'
  // Get custom token address from config file
  const customTokenAddress = getCustomTokenAddress();
  
  // Chain-specific configurations
  const chainConfigs = {
    consensus: {
      chainName: `consensus_${uuid}`,
      chainId: randomChainId,
      l1BatchCommitDataGeneratorMode: 'validium',
      baseTokenAddress: customTokenAddress,
      baseTokenPriceNominator: 314,
      baseTokenPriceDenominator: 1000,
      evmEmulator: false
    },
    validium: {
      chainName: `validium_${uuid}`,
      chainId: randomChainId,
      l1BatchCommitDataGeneratorMode: 'validium',
      baseTokenAddress: ethTokenAddress,
      baseTokenPriceNominator: 1,
      baseTokenPriceDenominator: 1,
      evmEmulator: true
    },
    da_migration: {
      chainName: `da_migration_${uuid}`,
      chainId: randomChainId,
      l1BatchCommitDataGeneratorMode: 'rollup',
      baseTokenAddress: ethTokenAddress,
      baseTokenPriceNominator: 1,
      baseTokenPriceDenominator: 1,
      evmEmulator: false
    },
    custom_token: {
      chainName: `custom_token_${uuid}`,
      chainId: randomChainId,
      l1BatchCommitDataGeneratorMode: 'rollup',
      baseTokenAddress: customTokenAddress,
      baseTokenPriceNominator: 314,
      baseTokenPriceDenominator: 1000,
      evmEmulator: false
    },
    era: {
      chainName: `era_${uuid}`,
      chainId: randomChainId,
      l1BatchCommitDataGeneratorMode: 'rollup',
      baseTokenAddress: ethTokenAddress,
      baseTokenPriceNominator: 1,
      baseTokenPriceDenominator: 1,
      evmEmulator: true
    }
  };
  
  const chainConfig = chainConfigs[chainType];
  if (!chainConfig) {
    throw new Error(`Unsupported chain type: ${chainType}`);
  }
  
  console.log(`Creating and initializing ${chainType} chain: ${chainConfig.chainName}...`);
  
  try {
    // Acquire mutex for the entire chain creation and initialization process
    console.log(`🔒 Acquiring mutex for chain creation and phase1 initialization of ${chainConfig.chainName}...`);
    await fileMutex.acquire();
    console.log(`✅ Mutex acquired for ${chainConfig.chainName}`);
    
    try {
      // Step 1: Create the chain (under mutex protection)
      console.log(`⏳ Creating chain: ${chainConfig.chainName}`);
      await executeCommand('zkstack', [
        'chain', 'create',
        '--update-submodules', 'false',
        '--chain-name', chainConfig.chainName,
        '--chain-id', chainConfig.chainId.toString(),
        '--prover-mode', 'no-proofs',
        '--wallet-creation', 'localhost',
        '--l1-batch-commit-data-generator-mode', chainConfig.l1BatchCommitDataGeneratorMode,
        '--base-token-address', chainConfig.baseTokenAddress!,
        '--base-token-price-nominator', chainConfig.baseTokenPriceNominator.toString(),
        '--base-token-price-denominator', chainConfig.baseTokenPriceDenominator.toString(),
        '--set-as-default', 'false',
        '--ignore-prerequisites',
        '--evm-emulator', chainConfig.evmEmulator.toString(),
        '--tight-ports',
        '--verbose'
      ], chainConfig.chainName, "main_node");
      console.log(`✅ Chain creation completed for ${chainConfig.chainName}`);
      
      // Step 2: Initialize the chain - Phase 1 (under mutex protection)
      console.log(`⏳ Phase 1 initialization for ${chainConfig.chainName}`);
      await executeCommand('zkstack', [
        'chain', 'init', '--phase1',
        '--deploy-paymaster',
        '--l1-rpc-url', finalConfig.l1RpcUrl!,
        '--server-db-url', finalConfig.serverDbUrl!,
        '--server-db-name', `zksync_server_localhost_${chainConfig.chainName}`,
        '--chain', chainConfig.chainName,
        '--validium-type', 'no-da',
        '--update-submodules', 'false',
        '--verbose'
      ], chainConfig.chainName, "main_node");
      console.log(`✅ Phase 1 initialization completed for ${chainConfig.chainName}`);
      
      console.log(`✅ Chain creation and Phase 1 initialization completed for ${chainConfig.chainName}`);
    } finally {
      fileMutex.release();
    }
    
    // Step 3: Initialize the chain - Phase 2 (without mutex protection)
    console.log(`⏳ Phase 2 initialization for ${chainConfig.chainName}`);
    await executeCommand('zkstack', [
      'chain', 'init', '--phase2',
      '--deploy-paymaster',
      '--l1-rpc-url', finalConfig.l1RpcUrl!,
      '--server-db-url', finalConfig.serverDbUrl!,
      '--server-db-name', `zksync_server_localhost_${chainConfig.chainName}`,
      '--chain', chainConfig.chainName,
      '--validium-type', 'no-da',
      '--update-submodules', 'false',
      '--verbose'
    ], chainConfig.chainName, 'main_node');
    console.log(`✅ Phase 2 initialization completed for ${chainConfig.chainName}`);
    
    // Step 4: Start the server
    console.log(`🚀 Starting server for ${chainConfig.chainName}...`);
    const serverHandle = await startServer(chainConfig.chainName);
    console.log(`✅ Server started successfully for ${chainConfig.chainName}`);

    console.log(`✅ Chain creation and initialization completed for ${chainConfig.chainName}`);
    
    return { chainName: chainConfig.chainName, serverHandle };
  } catch (error) {
    console.error(`❌ Failed to create chain ${chainConfig.chainName}:`, error);
    throw error;
  }
} 
