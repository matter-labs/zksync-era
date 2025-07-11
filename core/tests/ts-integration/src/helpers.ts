import * as fs from 'fs';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import * as hre from 'hardhat';
import { ZkSyncArtifact } from '@matterlabs/hardhat-zksync-solc/dist/src/types';
import * as path from 'path';
import { loadConfig } from 'utils/src/file-configs';

import { L2_BRIDGEHUB_ADDRESS } from '../src/constants';

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
 * Waits until a new L1 batch is created on ZKsync node.
 * This function attempts to trigger this action by sending an additional transaction,
 * however it may not be enough in some env (e.g. if some testnet is configured to utilize the block capacity).
 *
 * @param wallet Wallet to send transaction from. Should have enough balance to cover the fee.
 */
export async function waitForNewL1Batch(wallet: zksync.Wallet): Promise<zksync.types.TransactionReceipt> {
    const MAX_ATTEMPTS = 3;

    let txResponse: ethers.TransactionResponse | null = null;
    let txReceipt: ethers.TransactionReceipt | null = null;
    let nonce = Number(await wallet.getNonce());
    for (let i = 0; i < MAX_ATTEMPTS; i++) {
        // Send a dummy transaction and wait for it to execute. We override `maxFeePerGas` as the default ethers behavior
        // is to fetch `maxFeePerGas` from the latest sealed block and double it which is not enough for scenarios with
        // extreme gas price fluctuations.
        let gasPrice = await wallet.provider.getGasPrice();
        if (!txResponse || !txResponse.maxFeePerGas || txResponse.maxFeePerGas < gasPrice) {
            txResponse = await wallet
                .transfer({
                    to: wallet.address,
                    amount: 0,
                    overrides: { maxFeePerGas: gasPrice, nonce: nonce, maxPriorityFeePerGas: 0, type: 2 }
                })
                .catch((e) => {
                    // Unlike `waitForTransaction` below, these errors are not wrapped as `EthersError` for some reason
                    if (<Error>e.message.match(/Not enough gas/)) {
                        console.log(
                            `Transaction did not have enough gas, likely gas price went up (attempt ${i + 1}/${MAX_ATTEMPTS})`
                        );
                        return null;
                    } else if (<Error>e.message.match(/max fee per gas less than block base fee/)) {
                        console.log(
                            `Transaction's max fee per gas was lower than block base fee, likely gas price went up (attempt ${i + 1}/${MAX_ATTEMPTS})`
                        );
                        return null;
                    } else if (<Error>e.message.match(/nonce too low/)) {
                        if (!txResponse) {
                            // Our transaction was never accepted to the mempool with this nonce so it must have been used by another transaction.
                            return wallet.getNonce().then((newNonce) => {
                                console.log(
                                    `Transaction's nonce is too low, updating from ${nonce} to ${newNonce} (attempt ${i + 1}/${MAX_ATTEMPTS})`
                                );
                                nonce = newNonce;
                                return null;
                            });
                        } else {
                            console.log(
                                `Transaction's nonce is too low, likely previous attempt succeeded, waiting longer (attempt ${i + 1}/${MAX_ATTEMPTS})`
                            );
                            return txResponse;
                        }
                    } else {
                        return Promise.reject(e);
                    }
                });
            if (!txResponse) {
                continue;
            }
        } else {
            console.log('Gas price has not gone up, waiting longer');
        }
        txReceipt = await wallet.provider.waitForTransaction(txResponse.hash, 1, 3000).catch((e) => {
            if (ethers.isError(e, 'TIMEOUT')) {
                console.log(`Transaction timed out, potentially gas price went up (attempt ${i + 1}/${MAX_ATTEMPTS})`);
                return null;
            } else if (ethers.isError(e, 'UNKNOWN_ERROR') && e.message.match(/Not enough gas/)) {
                console.log(
                    `Transaction did not have enough gas, likely gas price went up (attempt ${i + 1}/${MAX_ATTEMPTS})`
                );
                return null;
            } else {
                return Promise.reject(e);
            }
        });
        if (txReceipt) {
            // Transaction got executed, so we can safely assume it will be sealed in the next batch
            break;
        }
    }
    if (!txReceipt) {
        throw new Error('Failed to force an L1 batch to seal');
    }
    // Invariant: even with 1 transaction, l1 batch must be eventually sealed, so this loop must exit.
    while (!(await wallet.provider.getTransactionReceipt(txReceipt.hash))?.l1BatchNumber) {
        await zksync.utils.sleep(wallet.provider.pollingInterval);
    }
    return (await wallet.provider.getTransactionReceipt(txReceipt.hash))!;
}

/**
 * Waits until the requested block is finalized.
 *
 * @param wallet Wallet to use to poll the server.
 * @param blockNumber Number of block.
 */
export async function waitUntilBlockFinalized(wallet: zksync.Wallet, blockNumber: number) {
    console.log('Waiting for block to be finalized...', blockNumber);
    let printedBlockNumber = 0;
    while (true) {
        const block = await wallet.provider.getBlock('finalized');
        if (blockNumber <= block.number) {
            break;
        } else {
            if (printedBlockNumber < block.number) {
                console.log('Waiting for block to be finalized...', blockNumber, block.number);
                console.log('time', new Date().toISOString());
                printedBlockNumber = block.number;
            }
            await zksync.utils.sleep(wallet.provider.pollingInterval);
        }
    }
}

// /**
//  * Waits until the requested block is finalized.
//  *
//  * @param wallet Wallet to use to poll the server.
//  * @param blockNumber Number of block.
//  */
// export async function waitUntilBlockExecutedOnGateway(
//     wallet: zksync.Wallet,
//     gwWallet: zksync.Wallet,
//     blockNumber: number
// ) {
//     // console.log('Waiting for block to be finalized...', blockNumber);
//     let batchNumber = (await wallet.provider.getBlockDetails(blockNumber)).l1BatchNumber;
//     let currentExecutedBatchNumber = 0;
//     while (currentExecutedBatchNumber < batchNumber) {
//         const bridgehub = new ethers.Contract(
//             L2_BRIDGEHUB_ADDRESS,
//             ['function getZKChain(uint256) view returns (address)'],
//             gwWallet
//         );
//         const zkChainAddr = await bridgehub.getZKChain(await wallet.provider.getNetwork().then((net) => net.chainId));
//         const gettersFacet = new ethers.Contract(
//             zkChainAddr,
//             ['function getTotalBatchesExecuted() view returns (uint256)'],
//             gwWallet
//         );
//         currentExecutedBatchNumber = await gettersFacet.getTotalBatchesExecuted();
//         // console.log('currentExecutedBatchNumber', currentExecutedBatchNumber);
//         // console.log('batchNumber awaited', batchNumber);
//         if (currentExecutedBatchNumber >= batchNumber) {
//             break;
//         } else {
//             await zksync.utils.sleep(wallet.provider.pollingInterval);
//         }
//     }
// }

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
    let i = 0;
    while ((await wallet.provider.getLogProof(txHash)) == null) {
        console.log('Waiting for log proof...', i);
        await zksync.utils.sleep(wallet.provider.pollingInterval);
        i++;
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

// Gets the L2-B provider URL based on the L2-A provider URL: validium (L2-B) for era (L2-A), or era (L2-B) for validium (L2-A)
export function getL2bUrl(chainName: string) {
    const pathToHome = path.join(__dirname, '../../../..');
    const config = loadConfig({
        pathToHome,
        chain: chainName,
        config: 'general.yaml'
    });
    const url = config.api.web3_json_rpc.http_url;
    return url;
}
