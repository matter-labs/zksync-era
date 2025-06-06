import { spawn as _spawn, ChildProcessWithoutNullStreams, type ProcessEnvOptions } from 'node:child_process';
import { assert, expect } from 'chai';
import { getAllConfigsPath, replaceL1BatchMinAgeBeforeExecuteSeconds } from 'utils/build/file-configs';
import { IZkSyncHyperchain } from 'zksync-ethers/build/typechain';
import { Tester } from './tester';
import { killPidWithAllChilds } from 'utils/build/kill';
import * as utils from 'utils';
import fs from 'node:fs/promises';
import * as path from 'node:path';
import * as os from 'node:os';
import * as zksync from 'zksync-ethers';

// executes a command in background and returns a child process handle
// by default pipes data to parent's stdio but this can be overridden
export function background({
    command,
    stdio = 'inherit',
    cwd
}: {
    command: string;
    stdio: any;
    cwd?: ProcessEnvOptions['cwd'];
}): ChildProcessWithoutNullStreams {
    command = command.replace(/\n/g, ' ');
    utils.log(`Run command ${command}`);
    return _spawn(command, { stdio: stdio, shell: true, detached: true, cwd });
}

export function runInBackground({
    command,
    components,
    stdio,
    cwd
}: {
    command: string;
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
}): ChildProcessWithoutNullStreams {
    if (components && components.length > 0) {
        command += ` --components=${components.join(',')}`;
    }
    return background({ command, stdio, cwd });
}

function runServerInBackground({
    components,
    stdio,
    cwd,
    chain
}: {
    components?: string[];
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    chain: string;
}): ChildProcessWithoutNullStreams {
    const command = `zkstack server --chain ${chain}`;
    return runInBackground({ command, components, stdio, cwd });
}

export function runExternalNodeInBackground({
    stdio,
    cwd,
    chain
}: {
    stdio: any;
    cwd?: Parameters<typeof background>[0]['cwd'];
    chain: string;
}): ChildProcessWithoutNullStreams {
    const command = `zkstack external-node run --chain ${chain}`;
    return runInBackground({ command, stdio, cwd });
}

async function exec(command: string, options: ProcessEnvOptions) {
    command = command.replace(/\n/g, ' ');
    utils.log(`Executing command: ${command}`);
    const childProcess = _spawn(command, { stdio: 'inherit', shell: true, ...options });
    await new Promise((resolve, reject) => {
        childProcess.on('exit', (exitCode) => {
            if (exitCode === 0) {
                resolve(undefined);
            } else {
                reject(new Error(`process exited with non-zero code: ${exitCode}`));
            }
        });
        childProcess.on('error', reject);
    });
}

export interface SuggestedValues {
    lastExecutedL1BatchNumber: bigint;
    nonce: number;
    priorityFee: number;
}

/** Parses output of "print-suggested-values" command of the revert block tool. */
export function parseSuggestedValues(jsonString: string): SuggestedValues {
    let json;
    try {
        json = JSON.parse(jsonString);
    } catch {
        utils.log(`Failed to parse string: ${jsonString}`);
    }
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

async function runBlockReverter(pathToHome: string, chain: string, args: string[]) {
    const configPaths = getAllConfigsPath({ pathToHome, chain });
    const fileConfigFlags = `
        --config-path=${configPaths['general.yaml']}
        --contracts-config-path=${configPaths['contracts.yaml']}
        --secrets-path=${configPaths['secrets.yaml']}
        --wallets-path=${configPaths['wallets.yaml']}
        --genesis-path=${configPaths['genesis.yaml']}
        --gateway-chain-path=${configPaths['gateway_chain.yaml']}
    `;

    const cmd = `cargo run --manifest-path ./core/Cargo.toml --bin block_reverter --release -- ${args.join(
        ' '
    )} ${fileConfigFlags}`;

    await exec(cmd, { cwd: pathToHome });
}

export async function revertExternalNode(chain: string, l1Batch: bigint) {
    const command = `zkstack external-node run --chain ${chain} -- revert ${l1Batch}`;
    await utils.spawn(command);
}

export async function executeRevert(
    pathToHome: string,
    chain: string,
    operatorAddress: string,
    batchesCommittedBeforeRevert: bigint,
    mainContract: IZkSyncHyperchain
): Promise<bigint> {
    const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'zksync-revert-test-'));
    const jsonPath = path.join(tmpDir, 'values.json');
    utils.log(`Temporary file for suggested revert values: ${jsonPath}`);

    let suggestedValuesOutput: string;
    try {
        await runBlockReverter(pathToHome, chain, [
            'print-suggested-values',
            '--json',
            jsonPath,
            '--operator-address',
            operatorAddress
        ]);
        suggestedValuesOutput = await fs.readFile(jsonPath, { encoding: 'utf-8' });
    } finally {
        await fs.rm(tmpDir, { recursive: true, force: true });
    }

    const values = parseSuggestedValues(suggestedValuesOutput);
    assert(
        values.lastExecutedL1BatchNumber < batchesCommittedBeforeRevert,
        'There should be at least one block for revert'
    );

    utils.log('Reverting with parameters', values);

    utils.log('Sending ETH transaction..');
    await runBlockReverter(pathToHome, chain, [
        'send-eth-transaction',
        '--l1-batch-number',
        values.lastExecutedL1BatchNumber.toString(),
        '--nonce',
        values.nonce.toString(),
        '--priority-fee-per-gas',
        values.priorityFee.toString()
    ]);

    utils.log('Rolling back DB..');
    await runBlockReverter(pathToHome, chain, [
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
    return values.lastExecutedL1BatchNumber;
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
            utils.log(`ignored error: ${err}`);
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
            utils.log(`ignored error: ${err}`);
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
        utils.log(`Initial L1 batch: ${initialL1BatchNumber}`);

        const depositHandle = await this.tester.syncWallet.deposit({
            token: this.tester.isETHBasedChain ? zksync.utils.LEGACY_ETH_ADDRESS : this.tester.baseTokenAddress,
            amount,
            to,
            approveBaseERC20: true,
            approveERC20: true
        });

        let depositBatchNumber;
        while (!(depositBatchNumber = (await depositHandle.wait()).l1BatchNumber)) {
            utils.log('Deposit is not included in L1 batch; sleeping');
            await utils.sleep(1);
        }
        utils.log(`Deposit was included into L1 batch ${depositBatchNumber}`);
        expect(depositBatchNumber).to.be.greaterThan(initialL1BatchNumber);
        return depositBatchNumber;
    }
}

export class NodeSpawner {
    public constructor(
        private readonly pathToHome: string,
        private readonly logs: fs.FileHandle,
        private readonly chainName: string,
        private readonly options: MainNodeSpawnOptions
    ) {}

    public async spawnMainNode(enableExecute: boolean): Promise<Node<NodeType.MAIN>> {
        const { chainName, pathToHome, options, logs } = this;

        replaceL1BatchMinAgeBeforeExecuteSeconds(pathToHome, chainName, enableExecute ? 0 : 10000);

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
            chain: chainName
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
        const { pathToHome, chainName, logs, options } = this;

        // Run server in background.
        let proc = runExternalNodeInBackground({
            stdio: ['ignore', logs, logs],
            cwd: pathToHome,
            chain: chainName
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
            utils.log(`Initialized node API on ${l2Url}; latest block: ${blockNumber}`);
            break;
        } catch (err) {
            if (proc.exitCode != null) {
                assert.fail(`server failed to start, exitCode = ${proc.exitCode}`);
            }
            utils.log(`Node waiting for API on ${l2Url}`);
            await utils.sleep(1);
        }
    }
}

export async function waitToExecuteBatch(mainContract: IZkSyncHyperchain, latestBatch: number) {
    let tryCount = 0;
    const initialExecutedBatch = await mainContract.getTotalBatchesExecuted();
    utils.log(`Initial executed L1 batch: ${initialExecutedBatch}`);

    if (initialExecutedBatch >= latestBatch) {
        utils.log('Latest batch is executed; no need to wait');
        return;
    }

    let lastExecutedBatch;
    while (
        (lastExecutedBatch = await mainContract.getTotalBatchesExecuted()) === initialExecutedBatch &&
        tryCount < 100
    ) {
        utils.log(`Last executed batch: ${lastExecutedBatch}`);
        tryCount++;
        await utils.sleep(1);
    }
    assert(lastExecutedBatch > initialExecutedBatch);
}

export async function waitToCommitBatchesWithoutExecution(mainContract: IZkSyncHyperchain): Promise<bigint> {
    let batchesCommitted = await mainContract.getTotalBatchesCommitted();
    let batchesExecuted = await mainContract.getTotalBatchesExecuted();
    utils.log(`Batches committed: ${batchesCommitted}, executed: ${batchesExecuted}`);

    let tryCount = 0;
    while ((batchesExecuted === 0n || batchesCommitted === batchesExecuted) && tryCount < 100) {
        await utils.sleep(1);
        batchesCommitted = await mainContract.getTotalBatchesCommitted();
        batchesExecuted = await mainContract.getTotalBatchesExecuted();
        utils.log(`Batches committed: ${batchesCommitted}, executed: ${batchesExecuted}`);
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
        utils.log(`Deposit ${depositHandle.hash} is not visible to the L1 network; sleeping`);
        await utils.sleep(1);
        l1TxResponse = await wallet._providerL1().getTransaction(depositHandle.hash);
    }
    utils.log(`Got L1 deposit tx`, l1TxResponse);

    // ethers doesn't work well with block reversions, so wait for the receipt before calling `.waitFinalize()`.
    const l2Tx = await wallet._providerL2().getL2TransactionFromPriorityOp(l1TxResponse);
    let receipt = null;
    while (receipt === null) {
        utils.log(`L2 deposit transaction ${l2Tx.hash} is not confirmed; sleeping`);
        await utils.sleep(1);
        receipt = await tester.syncWallet.provider.getTransactionReceipt(l2Tx.hash);
    }
    expect(receipt.status).to.be.eql(1);
    utils.log(`L2 deposit transaction ${l2Tx.hash} is confirmed`);

    await depositHandle.waitFinalize();
    utils.log('New deposit is finalized');
}

/** Returns sender's balance after the transfer is complete. */
export async function checkRandomTransfer(sender: zksync.Wallet, amount: bigint): Promise<bigint> {
    const senderBalanceBefore = await sender.getBalance();
    utils.log(`Sender's balance before transfer: ${senderBalanceBefore}`);

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
        utils.log(`Transfer ${transferHandle.hash} is not confirmed, sleeping`);
        await utils.sleep(1);
        txReceipt = await sender.provider.getTransactionReceipt(transferHandle.hash);
    }

    const senderBalance = await sender.getBalance();
    utils.log(`Sender's balance after transfer: ${senderBalance}`);
    const receiverBalance = await receiver.getBalance();
    utils.log(`Receiver's balance after transfer: ${receiverBalance}`);

    assert(receiverBalance === amount, 'Failed updated the balance of the receiver');

    const spentAmount = txReceipt.gasUsed * transferHandle.gasPrice! + amount;
    utils.log(`Expected spent amount: ${spentAmount}`);
    assert(senderBalance + spentAmount >= senderBalanceBefore, 'Failed to update the balance of the sender');
    return senderBalance;
}
