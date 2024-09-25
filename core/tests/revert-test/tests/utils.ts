import { exec as _exec, spawn as _spawn, ChildProcessWithoutNullStreams, type ProcessEnvOptions } from 'child_process';
import { promisify } from 'util';
import { assert, expect } from 'chai';
import { FileConfig, getAllConfigsPath, replaceAggregatedBlockExecuteDeadline } from 'utils/build/file-configs';
import { IZkSyncHyperchain } from 'zksync-ethers/build/typechain';
import { Tester } from './tester';
import { killPidWithAllChilds } from 'utils/build/kill';
import * as utils from 'utils';
import fs from 'node:fs/promises';
import * as zksync from 'zksync-ethers';

// executes a command in background and returns a child process handle
// by default pipes data to parent's stdio but this can be overridden
export function background({
    command,
    stdio = 'inherit',
    cwd,
    env
}: {
    command: string;
    stdio: any;
    cwd?: ProcessEnvOptions['cwd'];
    env?: ProcessEnvOptions['env'];
}): ChildProcessWithoutNullStreams {
    command = command.replace(/\n/g, ' ');
    console.log(`Run command ${command}`);
    return _spawn(command, { stdio: stdio, shell: true, detached: true, cwd, env });
}

export function runInBackground({
    command,
    components,
    stdio,
    cwd,
    env
}: {
    command: string;
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    env?: Parameters<typeof background>[0]['env'];
}): ChildProcessWithoutNullStreams {
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }
    return background({ command, stdio, cwd, env });
}

export function runServerInBackground({
    components,
    stdio,
    cwd,
    env,
    useZkInception,
    chain
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    env?: Parameters<typeof background>[0]['env'];
    useZkInception?: boolean;
    chain?: string;
}): ChildProcessWithoutNullStreams {
    let command = '';
    if (useZkInception) {
        command = 'zk_inception server';
        if (chain) {
            command += ` --chain ${chain}`;
        }
    } else {
        command = 'zk server';
    }
    return runInBackground({ command, components, stdio, cwd, env });
}

export function runExternalNodeInBackground({
    components,
    stdio,
    cwd,
    env,
    useZkInception,
    chain
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    env?: Parameters<typeof background>[0]['env'];
    useZkInception?: boolean;
    chain?: string;
}): ChildProcessWithoutNullStreams {
    let command = '';
    if (useZkInception) {
        command = 'zk_inception external-node run';
        command += chain ? ` --chain ${chain}` : '';
    } else {
        command = 'zk external-node';
    }

    return runInBackground({ command, components, stdio, cwd, env });
}

// async executor of shell commands
// spawns a new shell and can execute arbitrary commands, like "ls -la | grep .env"
// returns { stdout, stderr }
const promisified = promisify(_exec);

export function exec(command: string, options: ProcessEnvOptions) {
    command = command.replace(/\n/g, ' ');
    return promisified(command, options);
}

export interface SuggestedValues {
    lastExecutedL1BatchNumber: bigint;
    nonce: number;
    priorityFee: number;
}

/** Parses output of "print-suggested-values" command of the revert block tool. */
export function parseSuggestedValues(jsonString: string): SuggestedValues {
    const json = JSON.parse(jsonString);
    assert(json && typeof json === 'object');
    assert(Number.isInteger(json.last_executed_l1_batch_number));
    assert(Number.isInteger(json.nonce));
    assert(Number.isInteger(json.priority_fee));
    return {
        lastExecutedL1BatchNumber: BigInt(json.last_executed_l1_batch_number),
        nonce: json.nonce,
        priorityFee: json.priority_fee
    };
}

async function runBlockReverter(
    pathToHome: string,
    chain: string | undefined,
    env: ProcessEnvOptions['env'] | undefined,
    args: string[]
): Promise<string> {
    let fileConfigFlags = '';
    if (chain) {
        const configPaths = getAllConfigsPath({ pathToHome, chain });
        fileConfigFlags = `
            --config-path=${configPaths['general.yaml']}
            --contracts-config-path=${configPaths['contracts.yaml']}
            --secrets-path=${configPaths['secrets.yaml']}
            --wallets-path=${configPaths['wallets.yaml']}
            --genesis-path=${configPaths['genesis.yaml']}
        `;
    }

    const cmd = `cd ${pathToHome} && RUST_LOG=off cargo run --bin block_reverter --release -- ${args.join(
        ' '
    )} ${fileConfigFlags}`;

    const options = env
        ? {
              cwd: env.ZKSYNC_HOME,
              env: {
                  ...env,
                  PATH: process.env.PATH
              }
          }
        : {};
    const executedProcess = await exec(cmd, options);
    return executedProcess.stdout;
}

export async function executeRevert(
    pathToHome: string,
    chain: string | undefined,
    operatorAddress: string,
    batchesCommittedBeforeRevert: bigint,
    mainContract: IZkSyncHyperchain,
    env?: ProcessEnvOptions['env']
) {
    const suggestedValuesOutput = await runBlockReverter(pathToHome, chain, env, [
        'print-suggested-values',
        '--json',
        '--operator-address',
        operatorAddress
    ]);
    const values = parseSuggestedValues(suggestedValuesOutput);
    assert(
        values.lastExecutedL1BatchNumber < batchesCommittedBeforeRevert,
        'There should be at least one block for revert'
    );

    console.log('Reverting with parameters', values);

    console.log('Sending ETH transaction..');
    await runBlockReverter(pathToHome, chain, env, [
        'send-eth-transaction',
        '--l1-batch-number',
        values.lastExecutedL1BatchNumber.toString(),
        '--nonce',
        values.nonce.toString(),
        '--priority-fee-per-gas',
        values.priorityFee.toString()
    ]);

    console.log('Rolling back DB..');
    await runBlockReverter(pathToHome, chain, env, [
        'rollback-db',
        '--l1-batch-number',
        values.lastExecutedL1BatchNumber.toString(),
        '--rollback-postgres',
        '--rollback-tree',
        '--rollback-sk-cache',
        '--rollback-vm-runners-cache'
    ]);

    const blocksCommitted = await mainContract.getTotalBatchesCommitted();
    assert(blocksCommitted === values.lastExecutedL1BatchNumber, 'Revert on contract was unsuccessful');
}

export interface MainNodeSpawnOptions {
    enableConsensus: boolean;
    ethClientWeb3Url: string;
    apiWeb3JsonRpcHttpUrl: string;
    baseTokenAddress: string;
}

export enum NodeType {
    MAIN = 'zksync_server',
    EXT = 'zksync_external_node'
}

export class Node<TYPE extends NodeType> {
    constructor(
        public readonly tester: Tester,
        private readonly proc: ChildProcessWithoutNullStreams,
        private readonly type: TYPE
    ) {}

    public async terminate() {
        try {
            await killPidWithAllChilds(this.proc.pid!, 9);
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    /**
     * Terminates all main node processes running.
     *
     * WARNING: This is not safe to use when running nodes on multiple chains.
     */
    public static async killAll(type: NodeType) {
        try {
            await utils.exec(`killall -KILL ${type}`);
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    /** Waits for the node process to exit. */
    public async waitForExit(): Promise<number> {
        while (this.proc.exitCode === null) {
            await utils.sleep(1);
        }
        return this.proc.exitCode;
    }

    public async killAndWaitForShutdown() {
        await this.terminate();
        // Wait until it's really stopped.
        let iter = 0;
        while (iter < 30) {
            try {
                await this.tester.syncWallet.provider.getBlockNumber();
                await utils.sleep(2);
                iter += 1;
            } catch (_) {
                // When exception happens, we assume that server died.
                return;
            }
        }
        // It's going to panic anyway, since the server is a singleton entity, so better to exit early.
        throw new Error(`${this.type} didn't stop after a kill request`);
    }

    public async createBatchWithDeposit(to: string, amount: bigint) {
        const initialL1BatchNumber = await this.tester.web3Provider.getL1BatchNumber();
        console.log(`Initial L1 batch: ${initialL1BatchNumber}`);

        const depositHandle = await this.tester.syncWallet.deposit({
            token: this.tester.isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : this.tester.baseTokenAddress,
            amount,
            to,
            approveBaseERC20: true,
            approveERC20: true
        });

        let depositBatchNumber;
        while (!(depositBatchNumber = (await depositHandle.wait()).l1BatchNumber)) {
            console.log('Deposit is not included in L1 batch; sleeping');
            await utils.sleep(1);
        }
        console.log(`Deposit was included into L1 batch ${depositBatchNumber}`);
        expect(depositBatchNumber).to.be.greaterThan(initialL1BatchNumber);
        return depositBatchNumber;
    }
}

export class NodeSpawner {
    public constructor(
        private readonly pathToHome: string,
        private readonly logs: fs.FileHandle,
        private readonly fileConfig: FileConfig,
        private readonly options: MainNodeSpawnOptions,
        private readonly env?: ProcessEnvOptions['env']
    ) {}

    public async spawnMainNode(enableExecute: boolean): Promise<Node<NodeType.MAIN>> {
        const env = this.env ?? process.env;
        env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = enableExecute ? '1' : '10000';
        // Set full mode for the Merkle tree as it is required to get blocks committed.
        env.DATABASE_MERKLE_TREE_MODE = 'full';

        const { fileConfig, pathToHome, options, logs } = this;

        if (fileConfig.loadFromFile) {
            replaceAggregatedBlockExecuteDeadline(pathToHome, fileConfig, enableExecute ? 1 : 10000);
        }

        let components = 'api,tree,eth,state_keeper,commitment_generator,da_dispatcher,vm_runner_protective_reads';
        if (options.enableConsensus) {
            components += ',consensus';
        }
        if (options.baseTokenAddress != zksync.utils.LEGACY_ETH_ADDRESS) {
            components += ',base_token_ratio_persister';
        }
        let proc = runServerInBackground({
            components: [components],
            stdio: ['ignore', logs, logs],
            cwd: pathToHome,
            env: env,
            useZkInception: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });

        // Wait until the main node starts responding.
        const tester = await Tester.init(
            options.ethClientWeb3Url,
            options.apiWeb3JsonRpcHttpUrl,
            options.baseTokenAddress
        );
        await waitForNodeToStart(tester, proc, options.apiWeb3JsonRpcHttpUrl);
        return new Node(tester, proc, NodeType.MAIN);
    }

    public async spawnExtNode(): Promise<Node<NodeType.EXT>> {
        const env = this.env ?? process.env;
        const { pathToHome, fileConfig, logs, options } = this;

        let args = []; // FIXME: unused
        if (options.enableConsensus) {
            args.push('--enable-consensus');
        }

        // Run server in background.
        let proc = runExternalNodeInBackground({
            stdio: ['ignore', logs, logs],
            cwd: pathToHome,
            env,
            useZkInception: fileConfig.loadFromFile,
            chain: fileConfig.chain
        });

        const tester = await Tester.init(
            options.ethClientWeb3Url,
            options.apiWeb3JsonRpcHttpUrl,
            options.baseTokenAddress
        );
        await waitForNodeToStart(tester, proc, options.apiWeb3JsonRpcHttpUrl);
        return new Node(tester, proc, NodeType.EXT);
    }
}

async function waitForNodeToStart(tester: Tester, proc: ChildProcessWithoutNullStreams, l2Url: string) {
    while (true) {
        try {
            const blockNumber = await tester.syncWallet.provider.getBlockNumber();
            console.log(`Initialized node API on ${l2Url}; latest block: ${blockNumber}`);
            break;
        } catch (err) {
            if (proc.exitCode != null) {
                assert.fail(`server failed to start, exitCode = ${proc.exitCode}`);
            }
            console.log(`Node waiting for API on ${l2Url}`);
            await utils.sleep(1);
        }
    }
}

export async function waitToExecuteBatch(mainContract: IZkSyncHyperchain, latestBatch: number) {
    let tryCount = 0;
    const initialExecutedBatch = await mainContract.getTotalBatchesExecuted();
    console.log(`Initial executed L1 batch: ${initialExecutedBatch}`);

    if (initialExecutedBatch >= latestBatch) {
        console.log('Latest batch is executed; no need to wait');
        return;
    }

    let lastExecutedBatch;
    while (
        (lastExecutedBatch = await mainContract.getTotalBatchesExecuted()) === initialExecutedBatch &&
        tryCount < 100
    ) {
        console.log(`Last executed batch: ${lastExecutedBatch}`);
        tryCount++;
        await utils.sleep(1);
    }
    assert(lastExecutedBatch > initialExecutedBatch);
}

export async function waitToCommitBatchesWithoutExecution(mainContract: IZkSyncHyperchain): Promise<bigint> {
    let batchesCommitted = await mainContract.getTotalBatchesCommitted();
    let batchesExecuted = await mainContract.getTotalBatchesExecuted();
    console.log(`Batches committed: ${batchesCommitted}, executed: ${batchesExecuted}`);

    let tryCount = 0;
    while ((batchesExecuted === 0n || batchesCommitted === batchesExecuted) && tryCount < 100) {
        await utils.sleep(1);
        batchesCommitted = await mainContract.getTotalBatchesCommitted();
        batchesExecuted = await mainContract.getTotalBatchesExecuted();
        console.log(`Batches committed: ${batchesCommitted}, executed: ${batchesExecuted}`);
        tryCount += 1;
    }
    expect(batchesCommitted > batchesExecuted, 'There is no committed but not executed batch').to.be.true;
    return batchesCommitted;
}

export async function executeDepositAfterRevert(tester: Tester, wallet: zksync.Wallet, amount: bigint) {
    const depositHandle = await tester.syncWallet.deposit({
        token: tester.isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : tester.baseTokenAddress,
        amount,
        to: wallet.address,
        approveBaseERC20: true,
        approveERC20: true
    });

    let l1TxResponse = await wallet._providerL1().getTransaction(depositHandle.hash);
    while (!l1TxResponse) {
        console.log(`Deposit ${depositHandle.hash} is not visible to the L1 network; sleeping`);
        await utils.sleep(1);
        l1TxResponse = await wallet._providerL1().getTransaction(depositHandle.hash);
    }
    console.log(`Got L1 deposit tx`, l1TxResponse);

    // ethers doesn't work well with block reversions, so wait for the receipt before calling `.waitFinalize()`.
    const l2Tx = await wallet._providerL2().getL2TransactionFromPriorityOp(l1TxResponse);
    let receipt = null;
    while (receipt === null) {
        console.log(`L2 deposit transaction ${l2Tx.hash} is not confirmed; sleeping`);
        await utils.sleep(1);
        receipt = await tester.syncWallet.provider.getTransactionReceipt(l2Tx.hash);
    }
    expect(receipt.status).to.be.eql(1);
    console.log(`L2 deposit transaction ${l2Tx.hash} is confirmed`);

    await depositHandle.waitFinalize();
    console.log('New deposit is finalized');
}

export async function checkRandomTransfer(sender: zksync.Wallet, amount: bigint) {
    const senderBalanceBefore = await sender.getBalance();
    console.log(`Sender's balance before transfer: ${senderBalanceBefore}`);

    const receiverHD = zksync.Wallet.createRandom();
    const receiver = new zksync.Wallet(receiverHD.privateKey, sender.provider);
    const transferHandle = await sender.sendTransaction({
        to: receiver.address,
        value: amount,
        type: 0
    });

    // ethers doesn't work well with block reversions, so we poll for the receipt manually.
    let txReceipt = null;
    while (txReceipt === null) {
        console.log(`Transfer ${transferHandle.hash} is not confirmed, sleeping`);
        await utils.sleep(1);
        txReceipt = await sender.provider.getTransactionReceipt(transferHandle.hash);
    }

    const senderBalance = await sender.getBalance();
    console.log(`Sender's balance after transfer: ${senderBalance}`);
    const receiverBalance = await receiver.getBalance();
    console.log(`Receiver's balance after transfer: ${receiverBalance}`);

    assert(receiverBalance === amount, 'Failed updated the balance of the receiver');

    const spentAmount = txReceipt.gasUsed * transferHandle.gasPrice! + amount;
    console.log(`Expected spent amount: ${spentAmount}`);
    assert(senderBalance + spentAmount >= senderBalanceBefore, 'Failed to update the balance of the sender');
}
