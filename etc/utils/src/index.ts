import { exec as _exec, spawn as _spawn } from 'child_process';
import { promisify } from 'util';
export * from './node-spawner';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';

export type { ChildProcess } from 'child_process';

/** Logs information with additional data (e.g., a timestamp). */
export function log(message: string, ...args: any[]) {
    console.log(`[${new Date().toISOString()}] ${message}`, ...args);
}

// async executor of shell commands
// spawns a new shell and can execute arbitrary commands, like "ls -la | grep .env"
// returns { stdout, stderr }
const promisified = promisify(_exec);

export function exec(command: string) {
    command = command.replace(/\n/g, ' ');
    return promisified(command);
}

// executes a command in a new shell
// but pipes data to parent's stdout/stderr
export function spawn(command: string) {
    command = command.replace(/\n/g, ' ');
    log(`+ ${command}`);
    const child = _spawn(command, { stdio: 'inherit', shell: true });
    return new Promise((resolve, reject) => {
        child.on('error', reject);
        child.on('close', (code) => {
            code == 0 ? resolve(code) : reject(`Child process exited with code ${code}`);
        });
    });
}

export async function sleep(seconds: number) {
    return new Promise((resolve) => setTimeout(resolve, seconds * 1000));
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
