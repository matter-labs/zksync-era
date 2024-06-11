/**
 * Shared utils for recovery tests.
 */

import fs, { FileHandle } from 'node:fs/promises';
import fetch, { FetchError } from 'node-fetch';
import { promisify } from 'node:util';
import { ChildProcess, exec, spawn } from 'node:child_process';
import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import path from 'node:path';
import { expect } from 'chai';

export interface Health<T> {
    readonly status: string;
    readonly details?: T;
}

export interface SnapshotRecoveryDetails {
    readonly snapshot_l1_batch: number;
    readonly snapshot_l2_block: number;
    readonly factory_deps_recovered: boolean;
    readonly tokens_recovered: boolean;
    readonly storage_logs_chunks_left_to_process: number;
}

export interface ConsistencyCheckerDetails {
    readonly first_checked_batch?: number;
    readonly last_checked_batch?: number;
}

export interface ReorgDetectorDetails {
    readonly last_correct_l1_batch?: number;
    readonly last_correct_l2_block?: number;
}

export interface TreeDetails {
    readonly min_l1_batch_number?: number | null;
    readonly next_l1_batch_number?: number;
}

export interface DbPrunerDetails {
    readonly last_soft_pruned_l1_batch?: number;
    readonly last_hard_pruned_l1_batch?: number;
}

export interface TreeDataFetcherDetails {
    readonly last_updated_l1_batch?: number;
}

export interface HealthCheckResponse {
    readonly status: string;
    readonly components: {
        snapshot_recovery?: Health<SnapshotRecoveryDetails>;
        consistency_checker?: Health<ConsistencyCheckerDetails>;
        reorg_detector?: Health<ReorgDetectorDetails>;
        tree?: Health<TreeDetails>;
        db_pruner?: Health<DbPrunerDetails>;
        tree_pruner?: Health<{}>;
        tree_data_fetcher?: Health<TreeDataFetcherDetails>;
    };
}

export async function sleep(millis: number) {
    await new Promise((resolve) => setTimeout(resolve, millis));
}

export async function getExternalNodeHealth() {
    const EXTERNAL_NODE_HEALTH_URL = 'http://127.0.0.1:3081/health';

    try {
        const response: HealthCheckResponse = await fetch(EXTERNAL_NODE_HEALTH_URL).then((response) => response.json());
        return response;
    } catch (e) {
        let displayedError = e;
        if (e instanceof FetchError && e.code === 'ECONNREFUSED') {
            displayedError = '(connection refused)'; // Don't spam logs with "connection refused" messages
        }
        console.log(
            `Request to EN health check server failed: ${displayedError}. In CI, you can see more details ` +
                'in "Show * logs" steps'
        );
        return null;
    }
}

export async function dropNodeDatabase(env: { [key: string]: string }) {
    await executeNodeCommand(env, 'zk db reset');
}

export async function dropNodeStorage(env: { [key: string]: string }) {
    await executeNodeCommand(env, 'zk clean --database');
}

async function executeNodeCommand(env: { [key: string]: string }, command: string) {
    const childProcess = spawn(command, {
        cwd: process.env.ZKSYNC_HOME!!,
        stdio: 'inherit',
        shell: true,
        env
    });
    try {
        await waitForProcess(childProcess, true);
    } finally {
        childProcess.kill();
    }
}

export async function executeCommandWithLogs(command: string, logsPath: string) {
    const logs = await fs.open(logsPath, 'w');
    const childProcess = spawn(command, {
        cwd: process.env.ZKSYNC_HOME!!,
        stdio: [null, logs.fd, logs.fd],
        shell: true
    });
    try {
        await waitForProcess(childProcess, true);
    } finally {
        childProcess.kill();
        await logs.close();
    }
}

export enum NodeComponents {
    STANDARD = 'all',
    WITH_TREE_FETCHER = 'all,tree_fetcher',
    WITH_TREE_FETCHER_AND_NO_TREE = 'core,api,tree_fetcher'
}

function externalNodeArgs(components: NodeComponents = NodeComponents.STANDARD) {
    const enableConsensus = process.env.ENABLE_CONSENSUS === 'true';
    const args = ['external-node', '--', `--components=${components}`];
    if (enableConsensus) {
        args.push('--enable-consensus');
    }
    return args;
}

export class NodeProcess {
    static async stopAll(signal: 'INT' | 'KILL' = 'INT') {
        interface ChildProcessError extends Error {
            readonly code: number | null;
        }

        try {
            await promisify(exec)(`killall -q -${signal} zksync_external_node`);
        } catch (err) {
            const typedErr = err as ChildProcessError;
            if (typedErr.code === 1) {
                // No matching processes were found; this is fine.
            } else {
                throw err;
            }
        }
    }

    static async spawn(
        env: { [key: string]: string },
        logsFile: FileHandle | string,
        components: NodeComponents = NodeComponents.STANDARD
    ) {
        const logs = typeof logsFile === 'string' ? await fs.open(logsFile, 'w') : logsFile;
        const childProcess = spawn('zk', externalNodeArgs(components), {
            cwd: process.env.ZKSYNC_HOME!!,
            stdio: [null, logs.fd, logs.fd],
            shell: true,
            env
        });
        return new NodeProcess(childProcess, logs);
    }

    private constructor(private childProcess: ChildProcess, readonly logs: FileHandle) {}

    exitCode() {
        return this.childProcess.exitCode;
    }

    async stopAndWait(signal: 'INT' | 'KILL' = 'INT') {
        await NodeProcess.stopAll(signal);
        await waitForProcess(this.childProcess, signal === 'INT');
    }
}

async function waitForProcess(childProcess: ChildProcess, checkExitCode: boolean) {
    await new Promise((resolve, reject) => {
        childProcess.on('error', (error) => {
            reject(error);
        });
        childProcess.on('exit', (code) => {
            if (!checkExitCode || code === 0) {
                resolve(undefined);
            } else {
                reject(new Error(`Process exited with non-zero code: ${code}`));
            }
        });
    });
}

/**
 * Funded wallet wrapper that can be used to generate L1 batches.
 */
export class FundedWallet {
    static async create(mainNode: zksync.Provider, eth: ethers.Provider): Promise<FundedWallet> {
        const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant/eth.json`);
        const ethTestConfig = JSON.parse(await fs.readFile(testConfigPath, { encoding: 'utf-8' }));
        const mnemonic = ethers.Mnemonic.fromPhrase(ethTestConfig.test_mnemonic);
        const walletHD = ethers.HDNodeWallet.fromMnemonic(mnemonic, "m/44'/60'/0'/0/0");
        const wallet = new zksync.Wallet(walletHD.privateKey, mainNode, eth);

        return new FundedWallet(wallet);
    }

    private constructor(private readonly wallet: zksync.Wallet) {}

    /** Ensure that this wallet is funded on L2, depositing funds from L1 if necessary. */
    async ensureIsFunded() {
        const balance = await this.wallet.getBalance();
        const minExpectedBalance = ethers.parseEther('0.001');
        if (balance >= minExpectedBalance) {
            console.log('Wallet has acceptable balance on L2', balance);
            return;
        }

        const l1Balance = await this.wallet.getBalanceL1();
        expect(l1Balance >= minExpectedBalance, 'L1 balance of funded wallet is too small').to.be.true;

        const baseTokenAddress = await this.wallet.getBaseToken();
        const isETHBasedChain = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;
        const depositParams = {
            token: isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : baseTokenAddress,
            amount: minExpectedBalance,
            to: this.wallet.address,
            approveBaseERC20: true,
            approveERC20: true
        };
        console.log('Depositing funds on L2', depositParams);
        const depositTx = await this.wallet.deposit(depositParams);
        await depositTx.waitFinalize();
    }

    /** Generates at least one L1 batch by transfering funds to itself. */
    async generateL1Batch(): Promise<number> {
        const transactionResponse = await this.wallet.transfer({
            to: this.wallet.address,
            amount: 1,
            token: zksync.utils.ETH_ADDRESS
        });
        console.log('Generated a transaction from funded wallet', transactionResponse);
        const receipt = await transactionResponse.wait();
        console.log('Got finalized transaction receipt', receipt);

        // Wait until an L1 batch with the transaction is sealed.
        const pastL1BatchNumber = await this.wallet.provider.getL1BatchNumber();
        let newL1BatchNumber: number;
        while ((newL1BatchNumber = await this.wallet.provider.getL1BatchNumber()) <= pastL1BatchNumber) {
            await sleep(1000);
        }
        console.log(`Sealed L1 batch #${newL1BatchNumber}`);
        return newL1BatchNumber;
    }
}
