import * as fs from 'fs';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import * as hre from 'hardhat';
import { ZkSyncArtifact } from '@matterlabs/hardhat-zksync-solc/dist/src/types';

export const SYSTEM_CONTEXT_ADDRESS = '0x000000000000000000000000000000000000800b';

/**
 * Loads the test contract
 *
 * @param name Name of the contract, e.g. `Counter`
 * @returns Artifact containing the bytecode and ABI of the contract.
 */
export function getTestContract(name: string): ZkSyncArtifact {
    const artifact = hre.artifacts.readArtifactSync(name);
    return artifact as ZkSyncArtifact;
}

/**
 * Loads the `*.sol` file for a test contract.
 *
 * @param relativePath Path relative to the `ts-integration/contracts` folder (e.g. `contra).
 * @returns Conta
 */
export function getContractSource(relativePath: string): string {
    const contractPath = `${__dirname}/../contracts/${relativePath}`;
    const source = fs.readFileSync(contractPath, 'utf8');
    return source;
}

export function readContract(path: string, fileName: string) {
    return JSON.parse(fs.readFileSync(`${path}/${fileName}.sol/${fileName}.json`, { encoding: 'utf-8' }));
}

/**
 * Performs a contract deployment
 *
 * @param initiator Wallet from which contract should be deployed
 * @param artifact ABI and bytecode of the contract
 * @param args Constructor arguments for the contract
 * @param deploymentType Optional: should be set to `createAccount` if deployed contract would represent an account.
 * @param overrides Optional: overrides for the deployment transaction.
 * @returns Deployed contract object (with `initiator` wallet attached).
 */
export async function deployContract(
    initiator: zksync.Wallet,
    artifact: ZkSyncArtifact,
    args: any[],
    deploymentType?: zksync.types.DeploymentType,
    overrides: any = {}
): Promise<zksync.Contract> {
    const contractFactory = new zksync.ContractFactory(artifact.abi, artifact.bytecode, initiator, deploymentType);
    const contract = (await contractFactory.deploy(...args, overrides)) as zksync.Contract;
    await contract.waitForDeployment();
    return contract;
}

/**
 * Just performs a transaction. Can be used when you don't care about a particular action,
 * but just need a transaction to be executed.
 *
 * @param wallet Wallet to send a transaction from. Should have enough balance to cover the fee.
 * @returns Transaction receipt.
 */
export async function anyTransaction(wallet: zksync.Wallet): Promise<ethers.TransactionReceipt> {
    return await wallet.transfer({ to: wallet.address, amount: 0 }).then((tx) => tx.wait());
}

/**
 * Waits until the requested block is finalized.
 *
 * @param wallet Wallet to use to poll the server.
 * @param blockNumber Number of block.
 */
export async function waitUntilBlockFinalized(wallet: zksync.Wallet, blockNumber: number) {
    // console.log('Waiting for block to be finalized...', blockNumber);
    while (true) {
        const block = await wallet.provider.getBlock('finalized');
        if (blockNumber <= block.number) {
            break;
        } else {
            await zksync.utils.sleep(wallet.provider.pollingInterval);
        }
    }
}

export async function waitUntilBlockCommitted(wallet: zksync.Wallet, blockNumber: number) {
    console.log('Waiting for block to be committed...', blockNumber);
    while (true) {
        const block = await wallet.provider.getBlock('committed');
        if (blockNumber <= block.number) {
            break;
        } else {
            await zksync.utils.sleep(wallet.provider.pollingInterval);
        }
    }
}

async function getL1BatchFinalizationStatus(provider: zksync.Provider, number: number) {
    const result = await provider.send('zks_getL1BatchDetails', [number]);

    if (result == null) {
        return null;
    }
    if (result.executedAt != null) {
        return {
            finalizedHash: result.executeTxHash,
            finalizedAt: result.executedAt
        };
    }
    return null;
}

export async function waitForBlockToBeFinalizedOnL1(wallet: zksync.Wallet, blockNumber: number) {
    // Waiting for the block to be finalized on the immediate settlement layer.
    await waitUntilBlockFinalized(wallet, blockNumber);

    const provider = wallet.provider;

    const batchNumber = (await provider.getBlockDetails(blockNumber)).l1BatchNumber;

    let result = await getL1BatchFinalizationStatus(provider, batchNumber);

    while (result == null) {
        await zksync.utils.sleep(provider.pollingInterval);

        result = await getL1BatchFinalizationStatus(provider, batchNumber);
    }
}

export async function waitForL2ToL1LogProof(wallet: zksync.Wallet, blockNumber: number, txHash: string) {
    // First, we wait for block to be finalized.
    await waitUntilBlockFinalized(wallet, blockNumber);

    // Second, we wait for the log proof.
    while ((await wallet.provider.getLogProof(txHash)) == null) {
        // console.log('Waiting for log proof...');
        await zksync.utils.sleep(wallet.provider.pollingInterval);
    }
}

export async function getDeploymentNonce(provider: zksync.Provider, address: string): Promise<bigint> {
    const nonceHolder = new zksync.Contract(zksync.utils.NONCE_HOLDER_ADDRESS, zksync.utils.NONCE_HOLDER_ABI, provider);
    return await nonceHolder.getDeploymentNonce(address);
}

export async function getAccountNonce(provider: zksync.Provider, address: string): Promise<bigint> {
    const nonceHolder = new zksync.Contract(zksync.utils.NONCE_HOLDER_ADDRESS, zksync.utils.NONCE_HOLDER_ABI, provider);
    return await nonceHolder.getMinNonce(address);
}

/**
 * Returns an increased gas price to decrease chances of L1 transactions being stuck
 *
 * @param wallet Wallet to use to fetch the gas price.
 * @returns Scaled gas price.
 */
export async function scaledGasPrice(wallet: ethers.Wallet | zksync.Wallet): Promise<bigint> {
    const provider = wallet.provider;
    if (!provider) {
        throw new Error('Wallet should have provider');
    }
    const feeData = await provider.getFeeData();
    const gasPrice = feeData.gasPrice;
    if (!gasPrice) {
        throw new Error('Failed to fetch gas price');
    }
    // Increase by 40%
    return (gasPrice * 140n) / 100n;
}

export const bigIntReviver = (_: string, value: any) => {
    if (typeof value === 'string' && value.endsWith('n')) {
        const number = value.slice(0, -1);
        if (/^-?\d+$/.test(number)) {
            return BigInt(number);
        }
    }
    return value;
};

export const bigIntReplacer = (_: string, value: any) => {
    if (typeof value === 'bigint') {
        return `${value}n`;
    }
    return value;
};

export function bigIntMax(...args: bigint[]) {
    if (args.length === 0) {
        throw new Error('No arguments provided');
    }

    return args.reduce((max, current) => (current > max ? current : max), args[0]);
}

export function isLocalHost(network: string): boolean {
    return network.toLowerCase() == 'localhost';
}

export function maxL2GasLimitForPriorityTxs(maxGasBodyLimit: bigint): bigint {
    // Find maximum `gasLimit` that satisfies `txBodyGasLimit <= CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT`
    // using binary search.
    const overhead = getOverheadForTransaction(
        // We can just pass 0 as `encodingLength` because the overhead for the transaction's slot
        // will be greater than `overheadForLength` for a typical transacction
        0n
    );
    return maxGasBodyLimit + overhead;
}

export function getOverheadForTransaction(encodingLength: bigint): bigint {
    const TX_SLOT_OVERHEAD_GAS = 10_000n;
    const TX_LENGTH_BYTE_OVERHEAD_GAS = 10n;

    return bigIntMax(TX_SLOT_OVERHEAD_GAS, TX_LENGTH_BYTE_OVERHEAD_GAS * encodingLength);
}
