// Test of the behaviour of the external node when L1 batches get reverted.
//
// NOTE:
// main_contract.getTotalBlocksCommitted actually checks the number of batches committed.
// main_contract.getTotalBlocksExecuted actually checks the number of batches executed.
// TODO: Migrate from zksync-web3 to zksync-ethers.
import * as utils from 'zk/build/utils';
import { Tester } from './tester';
import * as zkweb3 from 'zksync-web3';
import { BigNumber, Contract, ethers } from 'ethers';
import { expect, assert } from 'chai';
import fs from 'fs';
import * as child_process from 'child_process';
import * as util from 'util';
import * as dotenv from 'dotenv';

// Parses output of "print-suggested-values" command of the revert block tool.
function parseSuggestedValues(suggestedValuesString: string) {
    const json = JSON.parse(suggestedValuesString);
    if (!json || typeof json !== 'object') {
        throw new TypeError('suggested values are not an object');
    }

    const lastL1BatchNumber = json.last_executed_l1_batch_number;
    if (!Number.isInteger(lastL1BatchNumber)) {
        throw new TypeError('suggested `lastL1BatchNumber` is not an integer');
    }
    const nonce = json.nonce;
    if (!Number.isInteger(nonce)) {
        throw new TypeError('suggested `nonce` is not an integer');
    }
    const priorityFee = json.priority_fee;
    if (!Number.isInteger(priorityFee)) {
        throw new TypeError('suggested `priorityFee` is not an integer');
    }

    return { lastL1BatchNumber, nonce, priorityFee };
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
    run(
        'cargo',
        ['build', '--release', '--bin', 'zksync_external_node', '--bin', 'zksync_server', '--bin', 'block_reverter'],
        { cwd: process.env.ZKSYNC_HOME }
    );
}

// Fetches env vars for the given environment (like 'dev', 'ext-node').
// TODO: it would be better to import zk tool code directly.
function fetchEnv(zksync_env: string): any {
    let res = run('./bin/zk', ['f', 'env'], {
        cwd: process.env.ZKSYNC_HOME,
        env: {
            ZKSYNC_ENV: zksync_env,
            ZKSYNC_HOME: process.env.ZKSYNC_HOME
        }
    });
    return { ...process.env, ...dotenv.parse(res.stdout) };
}

function runBlockReverter(args: string[]): string {
    let env = fetchEnv('docker');
    env.RUST_LOG = 'off';
    let res = run('./target/release/block_reverter', args, { cwd: env.ZKSYNC_HOME, env: env });
    console.log(res.stderr.toString());
    return res.stdout.toString();
}

class MainNode {
    constructor(public tester: Tester, private proc: child_process.ChildProcess) {}

    // Terminates all main node processes running.
    public static async terminate_all() {
        try {
            await utils.exec('killall -INT zksync_server --wait');
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    // Spawns a main node.
    // if enable_consensus is set, consensus component will be started in the main node.
    // if enable_execute is NOT set, main node will NOT send L1 transactions to execute L1 batches.
    public static async spawn(enable_consensus: boolean, enable_execute: boolean): Promise<MainNode> {
        let logs: fs.WriteStream = fs.createWriteStream('revert_main.log', { flags: 'a' });

        let env = fetchEnv('docker');
        env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = enable_execute ? '1' : '10000';
        // Set full mode for the Merkle tree as it is required to get blocks committed.
        env.DATABASE_MERKLE_TREE_MODE = 'full';
        console.log(`DATABASE_URL = ${env.DATABASE_URL}`);

        let components = 'api,tree,eth,state_keeper,commitment_generator';
        if (enable_consensus) {
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
        while (this.proc.exitCode == null) {
            await utils.sleep(1);
        }
        expect(this.proc.exitCode).to.equal(0);
    }
}

class ExtNode {
    constructor(public tester: Tester, private proc: child_process.ChildProcess) {}

    // Terminates all main node processes running.
    public static async terminate_all() {
        try {
            await utils.exec('killall -INT zksync_external_node --wait');
        } catch (err) {
            console.log(`ignored error: ${err}`);
        }
    }

    // Spawns an external node.
    // If enable_consensus is set, the node will use consensus P2P network to fetch blocks.
    public static async spawn(enable_consensus: boolean): Promise<ExtNode> {
        let logs: fs.WriteStream = fs.createWriteStream('revert_ext.log', { flags: 'a' });

        let env = fetchEnv('ext-node-docker');
        console.log(`DATABASE_URL = ${env.DATABASE_URL}`);
        let args = [];
        if (enable_consensus) {
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
        expect(await this.wait_for_exit()).to.equal(0);
    }

    // Waits for the node process to exit.
    public async wait_for_exit(): Promise<number> {
        while (this.proc.exitCode == null) {
            await utils.sleep(1);
        }
        return this.proc.exitCode;
    }
}

describe('Block reverting test', function () {
    compileBinaries();
    let enable_consensus = process.env.ENABLE_CONSENSUS == 'true';
    const depositAmount: BigNumber = ethers.utils.parseEther('0.001');

    step('run', async () => {
        console.log('Make sure that nodes are not running');
        await ExtNode.terminate_all();
        await MainNode.terminate_all();

        console.log('Start main node');
        let main_node = await MainNode.spawn(enable_consensus, true);
        console.log('Start ext node');
        let ext_node = await ExtNode.spawn(enable_consensus);
        let main_contract = await main_node.tester.syncWallet.getMainContract();
        let alice: zkweb3.Wallet = ext_node.tester.emptyWallet();

        console.log(
            'Finalize an L1 transaction to ensure at least 1 executed L1 batch and that all transactions are processed'
        );
        let h: zkweb3.types.PriorityOpResponse = await ext_node.tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });
        await h.waitFinalize();
        const alice1 = await alice.getBalance();

        console.log('Restart the main node with L1 batch execution disabled.');
        await main_node.terminate();
        main_node = await MainNode.spawn(enable_consensus, false);

        console.log('Commit at least 2 L1 batches which are not executed');
        let last_executed: BigNumber = await main_contract.getTotalBlocksExecuted();
        // One is not enough to test the reversion of sk cache because
        // it gets updated with some batch logs only at the start of the next batch.
        for (let i = 0; i < 2; i++) {
            let h: zkweb3.types.PriorityOpResponse = await ext_node.tester.syncWallet.deposit({
                token: zkweb3.utils.ETH_ADDRESS,
                amount: depositAmount,
                to: alice.address
            });
            await h.waitL1Commit();
        }
        while (true) {
            let last_committed: BigNumber = await main_contract.getTotalBlocksCommitted();
            console.log(`last_executed = ${last_executed}, last_committed = ${last_committed}`);
            if (last_committed.sub(last_executed).gte(2)) {
                break;
            }
            await utils.sleep(1);
        }
        const alice2 = await alice.getBalance();
        console.log('Terminate the main node');
        await main_node.terminate();

        console.log('Ask block_reverter to suggest to which L1 batch we should revert');
        let revert_env = { ...process.env };
        revert_env.RUST_LOG = 'off';
        const values_json = runBlockReverter(['print-suggested-values', '--json']);
        console.log(`values = ${values_json}`);
        let values = parseSuggestedValues(values_json);
        assert(last_executed == values.lastL1BatchNumber);

        console.log('Send reverting transaction to L1');
        runBlockReverter([
            'send-eth-transaction',
            '--l1-batch-number',
            values.lastL1BatchNumber,
            '--nonce',
            values.nonce,
            '--priority-fee-per-gas',
            values.priorityFee
        ]);

        console.log('Check that batches are reverted on L1');
        let last_committed2 = await main_contract.getTotalBlocksCommitted();
        console.log(`last_committed = ${last_committed2}, want ${last_executed}`);
        assert(last_committed2.eq(last_executed));

        console.log('Rollback db');
        runBlockReverter([
            'rollback-db',
            '--l1-batch-number',
            values.lastL1BatchNumber,
            '--rollback-postgres',
            '--rollback-tree',
            '--rollback-sk-cache'
        ]);

        console.log('Start main node.');
        main_node = await MainNode.spawn(enable_consensus, true);

        console.log('Wait for the external node to detect reorg and terminate');
        await ext_node.wait_for_exit();

        console.log('Restart external node and wait for it to revert.');
        ext_node = await ExtNode.spawn(enable_consensus);

        console.log('Execute an L1 transaction');
        const depositHandle = await ext_node.tester.syncWallet.deposit({
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
            receipt = await ext_node.tester.syncWallet.provider.getTransactionReceipt(l2Tx.hash);
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
        await MainNode.terminate_all();
        await ExtNode.terminate_all();
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
    } while (txReceipt == null);

    const senderBalance = await sender.getBalance();
    const receiverBalance = await receiver.getBalance();

    expect(receiverBalance.eq(amount), 'Failed updated the balance of the receiver').to.be.true;

    const spentAmount = txReceipt.gasUsed.mul(transferHandle.gasPrice!).add(amount);
    expect(senderBalance.add(spentAmount).eq(senderBalanceBefore), 'Failed to update the balance of the sender').to.be
        .true;
}
