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
import { runExternalNodeInBackground } from './utils';
import { killPidWithAllChilds } from 'utils/build/kill';

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

export async function getExternalNodeHealth(url: string) {
    try {
        const response: HealthCheckResponse = await fetch(url).then((response) => response.json());
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

export async function dropNodeData(env: { [key: string]: string }, useZkSupervisor?: boolean, chain?: string) {
    if (useZkSupervisor) {
        let cmd = 'zk_inception external-node init';
        cmd += chain ? ` --chain ${chain}` : '';
        await executeNodeCommand(env, cmd);
    } else {
        await executeNodeCommand(env, 'zk db reset');
        await executeNodeCommand(env, 'zk clean --database');
    }
}

async function executeNodeCommand(env: { [key: string]: string }, command: string) {
    const childProcess = spawn(command, {
        cwd: process.env.ZKSYNC_HOME!!,
        stdio: 'inherit',
        shell: true,
        env
    });
    try {
        await waitForProcess(childProcess);
    } finally {
        childProcess.kill();
    }
}

export async function executeCommandWithLogs(command: string, logsPath: string) {
    const logs = await fs.open(logsPath, 'w');
    const childProcess = spawn(command, {
        cwd: process.env.ZKSYNC_HOME!!,
        stdio: ['ignore', logs.fd, logs.fd],
        shell: true
    });
    try {
        await waitForProcess(childProcess);
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

    async stop(signal: 'INT' | 'KILL' = 'INT') {
        interface ChildProcessError extends Error {
            readonly code: number | null;
        }

        let signalNumber;
        if (signal == 'KILL') {
            signalNumber = 9;
        } else {
            signalNumber = 15;
        }
        try {
            await killPidWithAllChilds(this.childProcess.pid!, signalNumber);
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
        pathToHome: string,
        components: NodeComponents = NodeComponents.STANDARD,
        useZkInception?: boolean,
        chain?: string
    ) {
        const logs = typeof logsFile === 'string' ? await fs.open(logsFile, 'a') : logsFile;

        let childProcess = runExternalNodeInBackground({
            components: [components],
            stdio: ['ignore', logs.fd, logs.fd],
            cwd: pathToHome,
            env,
            useZkInception,
            chain
        });

        return new NodeProcess(childProcess, logs);
    }

    private constructor(private childProcess: ChildProcess, readonly logs: FileHandle) {}

    exitCode() {
        return this.childProcess.exitCode;
    }

    async stopAndWait(signal: 'INT' | 'KILL' = 'INT') {
        let processWait = waitForProcess(this.childProcess);
        await this.stop(signal);
        await processWait;
        console.log('stopped');
    }
}

function waitForProcess(childProcess: ChildProcess): Promise<any> {
    return new Promise((resolve, reject) => {
        childProcess.on('close', (_code, _signal) => {
            resolve(undefined);
        });
        childProcess.on('error', (error) => {
            reject(error);
        });
        childProcess.on('exit', (_code) => {
            resolve(undefined);
        });
        childProcess.on('disconnect', () => {
            resolve(undefined);
        });
    });
}

/**
 * Funded wallet wrapper that can be used to generate L1 batches.
 */
export class FundedWallet {
    static async create(mainNode: zksync.Provider, eth: ethers.Provider): Promise<FundedWallet> {
        if (!process.env.MASTER_WALLET_PK) {
            const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant/eth.json`);
            const ethTestConfig = JSON.parse(await fs.readFile(testConfigPath, { encoding: 'utf-8' }));
            const mnemonic = ethers.Mnemonic.fromPhrase(ethTestConfig.test_mnemonic);
            const walletHD = ethers.HDNodeWallet.fromMnemonic(mnemonic, "m/44'/60'/0'/0/0");

            process.env.MASTER_WALLET_PK = walletHD.privateKey;
        }

        const wallet = new zksync.Wallet(process.env.MASTER_WALLET_PK, mainNode, eth);

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

    /** Generates at least one L1 batch by transferring funds to itself. */
    async generateL1Batch(): Promise<number> {
        const transactionResponse = await this.wallet.transfer({
            to: this.wallet.address,
            amount: 1,
            token: zksync.utils.ETH_ADDRESS
        });
        console.log('Generated a transaction from funded wallet', transactionResponse);

        let receipt: zksync.types.TransactionReceipt;
        while (!(receipt = await transactionResponse.wait()).l1BatchNumber) {
            console.log('Transaction is not included in L1 batch; sleeping');
            await sleep(1000);
        }

        console.log('Got finalized transaction receipt', receipt);
        const newL1BatchNumber = receipt.l1BatchNumber;
        console.log(`Sealed L1 batch #${newL1BatchNumber}`);
        return newL1BatchNumber;
    }
}
