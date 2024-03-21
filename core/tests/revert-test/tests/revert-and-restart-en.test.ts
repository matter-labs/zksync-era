// Test of the behaviour of the external node when L1 batches get reverted.
//
// NOTE:
// main_contract.getTotalBlocksCommitted actually checks the number of batches committed.
// main_contract.getTotalBlocksExecuted actually checks the number of batches executed.
// TODO: Migrate from zksync-web3 to zksync-ethers.
import * as utils from 'zk/build/utils';
import { Tester } from './tester';
import * as zkweb3 from 'zksync-web3';
import { BigNumber, ethers } from 'ethers';
import { expect, assert } from 'chai';
import fs from 'fs';
import * as child_process from 'child_process';
import * as dotenv from 'dotenv';

const mainEnv: string = process.env.IN_DOCKER ? 'docker' : 'dev';
const extEnv: string = process.env.IN_DOCKER ? 'ext-node-docker' : 'ext-node';
const mainLogsPath: string = 'revert_main.log';
const extLogsPath: string = 'revert_ext.log';

interface SuggestedValues {
    lastExecutedL1BatchNumber: BigNumber;
    nonce: number;
    priorityFee: number;
}

// Parses output of "print-suggested-values" command of the revert block tool.
function parseSuggestedValues(jsonString: string): SuggestedValues {
    const json = JSON.parse(jsonString);
    assert(json && typeof json === 'object');
    assert(Number.isInteger(json.last_executed_l1_batch_number));
    assert(Number.isInteger(json.nonce));
    assert(Number.isInteger(json.priority_fee));
    return {
        lastExecutedL1BatchNumber: BigNumber.from(json.last_executed_l1_batch_number),
        nonce: json.nonce,
        priorityFee: json.priority_fee
    };
}

function spawn(cmd: string, args: string[], options: child_process.SpawnOptions): child_process.ChildProcess {
    return child_process.spawn(cmd, args, options);
}

function run(cmd: string, args: string[], options: child_process.SpawnOptions): child_process.SpawnSyncReturns<Buffer> {
    let res = child_process.spawnSync(cmd, args, options);
    expect(res.error).to.be.undefined;
    return res;
}

function compileBinaries() {
    console.log('compiling binaries');
    run(
        'cargo',
        ['build', '--release', '--bin', 'zksync_external_node', '--bin', 'zksync_server', '--bin', 'block_reverter'],
        { cwd: process.env.ZKSYNC_HOME }
    );
}

// Fetches env vars for the given environment (like 'dev', 'ext-node').
// TODO: it would be better to import zk tool code directly.
function fetchEnv(zksyncEnv: string): any {
    let res = run('./bin/zk', ['f', 'env'], {
        cwd: process.env.ZKSYNC_HOME,
        env: {
            ZKSYNC_ENV: zksyncEnv,
            ZKSYNC_HOME: process.env.ZKSYNC_HOME
        }
    });
    return { ...process.env, ...dotenv.parse(res.stdout) };
}

function runBlockReverter(args: string[]): string {
    let env = fetchEnv(mainEnv);
    env.RUST_LOG = 'off';
    let res = run('./target/release/block_reverter', args, { cwd: env.ZKSYNC_HOME, env: env });
    console.log(res.stderr.toString());
    return res.stdout.toString();
}

class MainNode {
    constructor(public tester: Tester, private proc: child_process.ChildProcess) {}

    // Terminates all main node processes running.
    public static async terminateAll() {
        try {
            await utils.exec('killall -INT zksync_server --wait');
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    // Spawns a main node.
    // if enableConsensus is set, consensus component will be started in the main node.
    // if enableExecute is NOT set, main node will NOT send L1 transactions to execute L1 batches.
    public static async spawn(
        logs: fs.WriteStream,
        enableConsensus: boolean,
        enableExecute: boolean
    ): Promise<MainNode> {
        let env = fetchEnv(mainEnv);
        env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = enableExecute ? '1' : '10000';
        // Set full mode for the Merkle tree as it is required to get blocks committed.
        env.DATABASE_MERKLE_TREE_MODE = 'full';
        console.log(`DATABASE_URL = ${env.DATABASE_URL}`);

        let components = 'api,tree,eth,state_keeper,commitment_generator';
        if (enableConsensus) {
            components += ',consensus';
        }
        let proc = spawn('./target/release/zksync_server', ['--components', components], {
            cwd: env.ZKSYNC_HOME,
            stdio: [null, logs, logs],
            env: env
        });
        // Wait until the main node starts responding.
        let tester: Tester = await Tester.init(env.ETH_CLIENT_WEB3_URL, env.API_WEB3_JSON_RPC_HTTP_URL);
        while (true) {
            try {
                await tester.syncWallet.provider.getBlockNumber();
                break;
            } catch (err) {
                if (proc.exitCode != null) {
                    assert.fail(`server failed to start, exitCode = ${proc.exitCode}`);
                }
                console.log('waiting for api endpoint');
                await utils.sleep(1);
            }
        }
        return new MainNode(tester, proc);
    }

    // Sends SIGINT to the main node process and waits for it to exit.
    public async terminate(): Promise<void> {
        this.proc.kill('SIGINT');
        while (this.proc.exitCode === null) {
            await utils.sleep(1);
        }
        expect(this.proc.exitCode).to.equal(0);
    }
}

class ExtNode {
    constructor(public tester: Tester, private proc: child_process.ChildProcess) {}

    // Terminates all main node processes running.
    public static async terminateAll() {
        try {
            await utils.exec('killall -INT zksync_external_node --wait');
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    // Spawns an external node.
    // If enableConsensus is set, the node will use consensus P2P network to fetch blocks.
    public static async spawn(logs: fs.WriteStream, enableConsensus: boolean): Promise<ExtNode> {
        let env = fetchEnv(extEnv);
        console.log(`DATABASE_URL = ${env.DATABASE_URL}`);
        let args = [];
        if (enableConsensus) {
            args.push('--enable-consensus');
        }
        let proc = spawn('./target/release/zksync_external_node', args, {
            cwd: env.ZKSYNC_HOME,
            stdio: [null, logs, logs],
            env: env
        });
        // Wait until the node starts responding.
        let tester: Tester = await Tester.init(env.EN_ETH_CLIENT_URL, `http://localhost:${env.EN_HTTP_PORT}`);
        while (true) {
            try {
                await tester.syncWallet.provider.getBlockNumber();
                break;
            } catch (err) {
                if (proc.exitCode != null) {
                    assert.fail(`node failed to start, exitCode = ${proc.exitCode}`);
                }
                console.log('waiting for api endpoint');
                await utils.sleep(1);
            }
        }
        return new ExtNode(tester, proc);
    }

    // Sends SIGINT to the node process and waits for it to exit.
    public async terminate(): Promise<void> {
        this.proc.kill('SIGINT');
        expect(await this.waitForExit()).to.equal(0);
    }

    // Waits for the node process to exit.
    public async waitForExit(): Promise<number> {
        while (this.proc.exitCode === null) {
            await utils.sleep(1);
        }
        return this.proc.exitCode;
    }
}

describe('Block reverting test', function () {
    if (process.env.SKIP_COMPILATION !== 'true') {
        compileBinaries();
    }
    console.log(`PWD = ${process.env.PWD}`);
    const mainLogs: fs.WriteStream = fs.createWriteStream(mainLogsPath, { flags: 'a' });
    const extLogs: fs.WriteStream = fs.createWriteStream(extLogsPath, { flags: 'a' });
    const enableConsensus = process.env.ENABLE_CONSENSUS === 'true';
    console.log(`enableConsensus = ${enableConsensus}`);
    const depositAmount: BigNumber = ethers.utils.parseEther('0.001');

    step('run', async () => {
        console.log('Make sure that nodes are not running');
        await ExtNode.terminateAll();
        await MainNode.terminateAll();

        console.log('Start main node');
        let mainNode = await MainNode.spawn(mainLogs, enableConsensus, true);
        console.log('Start ext node');
        let extNode = await ExtNode.spawn(extLogs, enableConsensus);
        const main_contract = await mainNode.tester.syncWallet.getMainContract();
        const alice: zkweb3.Wallet = extNode.tester.emptyWallet();

        console.log(
            'Finalize an L1 transaction to ensure at least 1 executed L1 batch and that all transactions are processed'
        );
        const h: zkweb3.types.PriorityOpResponse = await extNode.tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });
        await h.waitFinalize();

        console.log('Restart the main node with L1 batch execution disabled.');
        await mainNode.terminate();
        mainNode = await MainNode.spawn(mainLogs, enableConsensus, false);

        console.log('Commit at least 2 L1 batches which are not executed');
        const lastExecuted: BigNumber = await main_contract.getTotalBlocksExecuted();
        // One is not enough to test the reversion of sk cache because
        // it gets updated with some batch logs only at the start of the next batch.
        const initialL1BatchNumber = (await main_contract.getTotalBlocksCommitted()).toNumber();
        const firstDepositHandle = await extNode.tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });

        await firstDepositHandle.wait();
        while ((await extNode.tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber) {
            await utils.sleep(0.1);
        }

        const secondDepositHandle = await extNode.tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });
        await secondDepositHandle.wait();
        while ((await extNode.tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber + 1) {
            await utils.sleep(0.3);
        }

        while (true) {
            const lastCommitted: BigNumber = await main_contract.getTotalBlocksCommitted();
            console.log(`lastExecuted = ${lastExecuted}, lastCommitted = ${lastCommitted}`);
            if (lastCommitted.sub(lastExecuted).gte(2)) {
                break;
            }
            await utils.sleep(0.3);
        }
        const alice2 = await alice.getBalance();
        console.log('Terminate the main node');
        await mainNode.terminate();

        console.log('Ask block_reverter to suggest to which L1 batch we should revert');
        const values_json = runBlockReverter(['print-suggested-values', '--json']);
        console.log(`values = ${values_json}`);
        const values = parseSuggestedValues(values_json);
        assert(lastExecuted.eq(values.lastExecutedL1BatchNumber));

        console.log('Send reverting transaction to L1');
        runBlockReverter([
            'send-eth-transaction',
            '--l1-batch-number',
            values.lastExecutedL1BatchNumber.toString(),
            '--nonce',
            values.nonce.toString(),
            '--priority-fee-per-gas',
            values.priorityFee.toString()
        ]);

        console.log('Check that batches are reverted on L1');
        const lastCommitted2 = await main_contract.getTotalBlocksCommitted();
        console.log(`lastCommitted = ${lastCommitted2}, want ${lastExecuted}`);
        assert(lastCommitted2.eq(lastExecuted));

        console.log('Rollback db');
        runBlockReverter([
            'rollback-db',
            '--l1-batch-number',
            values.lastExecutedL1BatchNumber.toString(),
            '--rollback-postgres',
            '--rollback-tree',
            '--rollback-sk-cache'
        ]);

        console.log('Start main node.');
        mainNode = await MainNode.spawn(mainLogs, enableConsensus, true);

        console.log('Wait for the external node to detect reorg and terminate');
        await extNode.waitForExit();

        console.log('Restart external node and wait for it to revert.');
        extNode = await ExtNode.spawn(extLogs, enableConsensus);

        console.log('Execute an L1 transaction');
        const depositHandle = await extNode.tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });
        const l1TxResponse = await alice._providerL1().getTransaction(depositHandle.hash);
        // TODO: it would be nice to know WHY it "doesn't work well with block reversions" and what it actually means.
        console.log(
            "ethers doesn't work well with block reversions, so wait for the receipt before calling `.waitFinalize()`."
        );
        const l2Tx = await alice._providerL2().getL2TransactionFromPriorityOp(l1TxResponse);
        let receipt = null;
        while (true) {
            receipt = await extNode.tester.syncWallet.provider.getTransactionReceipt(l2Tx.hash);
            if (receipt != null) {
                break;
            }
            await utils.sleep(1);
        }
        await depositHandle.waitFinalize();
        expect(receipt.status).to.be.eql(1);

        // The reverted transactions are expected to be reexecuted before the next transaction is applied.
        // Hence we compare the state against the alice2, rather than against alice3.
        const alice4want = alice2.add(BigNumber.from(depositAmount));
        const alice4 = await alice.getBalance();
        console.log(`Alice's balance is ${alice4}, want ${alice4want}`);
        assert(alice4.eq(alice4want));

        console.log('Execute an L2 transaction');
        await checkedRandomTransfer(alice, BigNumber.from(1));
    });

    after('Terminate nodes', async () => {
        await MainNode.terminateAll();
        await ExtNode.terminateAll();
    });
});

// Transfers amount from sender to a random wallet in an L2 transaction.
async function checkedRandomTransfer(sender: zkweb3.Wallet, amount: BigNumber) {
    const senderBalanceBefore = await sender.getBalance();
    const receiver = zkweb3.Wallet.createRandom().connect(sender.provider);
    const transferHandle = await sender.sendTransaction({ to: receiver.address, value: amount });

    // ethers doesn't work well with block reversions, so we poll for the receipt manually.
    let txReceipt = null;
    do {
        txReceipt = await sender.provider.getTransactionReceipt(transferHandle.hash);
        await utils.sleep(1);
    } while (txReceipt === null);

    const senderBalance = await sender.getBalance();
    const receiverBalance = await receiver.getBalance();

    expect(receiverBalance.eq(amount), 'Failed updated the balance of the receiver').to.be.true;

    const spentAmount = txReceipt.gasUsed.mul(transferHandle.gasPrice!).add(amount);
    expect(senderBalance.add(spentAmount).eq(senderBalanceBefore), 'Failed to update the balance of the sender').to.be
        .true;
}
