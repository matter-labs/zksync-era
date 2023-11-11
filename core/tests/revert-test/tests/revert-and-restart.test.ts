import * as utils from 'zk/build/utils';
import { Tester } from './tester';
import * as zkweb3 from 'zksync-web3';
import { BigNumber, Contract, ethers } from 'ethers';
import { expect } from 'chai';
import fs from 'fs';

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

async function killServerAndWaitForShutdown(tester: Tester) {
    await utils.exec('pkill -9 zksync_server');
    // Wait until it's really stopped.
    let iter = 0;
    while (iter < 30) {
        try {
            await tester.syncWallet.provider.getBlockNumber();
            await utils.sleep(5);
            iter += 1;
        } catch (_) {
            // When exception happens, we assume that server died.
            return;
        }
    }
    // It's going to panic anyway, since the server is a singleton entity, so better to exit early.
    throw new Error("Server didn't stop after a kill request");
}

function ignoreError(_err: any, context?: string) {
    const message = context ? `Error ignored (context: ${context}).` : 'Error ignored.';
    console.info(message);
}

const depositAmount = ethers.utils.parseEther('0.001');

describe('Block reverting test', function () {
    let tester: Tester;
    let alice: zkweb3.Wallet;
    let mainContract: Contract;
    let blocksCommittedBeforeRevert: number;
    let logs: fs.WriteStream;

    before('create test wallet', async () => {
        tester = await Tester.init(process.env.CHAIN_ETH_NETWORK || 'localhost');
        alice = tester.emptyWallet();
        logs = fs.createWriteStream('revert.log', { flags: 'a' });
    });

    step('run server and execute some transactions', async () => {
        // Make sure server isn't running.
        await killServerAndWaitForShutdown(tester).catch(ignoreError);

        // Set 1000 seconds deadline for `ExecuteBlocks` operation.
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = '1000';
        // Set full mode for the Merkle tree as it is required to get blocks committed.
        process.env.DATABASE_MERKLE_TREE_MODE = 'full';

        // Run server in background.
        const components = 'api,tree,eth,data_fetcher,state_keeper';
        utils.background(`zk server --components ${components}`, [null, logs, logs]);
        // Server may need some time to recompile if it's a cold run, so wait for it.
        let iter = 0;
        while (iter < 30 && !mainContract) {
            try {
                mainContract = await tester.syncWallet.getMainContract();
            } catch (err) {
                ignoreError(err, 'waiting for server HTTP JSON-RPC to start');
                await utils.sleep(5);
                iter += 1;
            }
        }
        if (!mainContract) {
            throw new Error('Server did not start');
        }

        // Seal 2 L1 batches.
        // One is not enough to test the reversion of sk cache because
        // it gets updated with some batch logs only at the start of the next batch.
        const initialL1BatchNumber = await tester.web3Provider.getL1BatchNumber();

        const firstDepositHandle = await tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });
        await firstDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber) {
            await utils.sleep(1);
        }

        const secondDepositHandle = await tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });
        await secondDepositHandle.wait();
        while ((await tester.web3Provider.getL1BatchNumber()) <= initialL1BatchNumber + 1) {
            await utils.sleep(1);
        }

        const balance = await alice.getBalance();
        expect(balance.eq(depositAmount.mul(2)), 'Incorrect balance after deposits').to.be.true;

        // Check L1 committed and executed blocks.
        let blocksCommitted = await mainContract.getTotalBlocksCommitted();
        let blocksExecuted = await mainContract.getTotalBlocksExecuted();
        let tryCount = 0;
        while (blocksCommitted.eq(blocksExecuted) && tryCount < 10) {
            blocksCommitted = await mainContract.getTotalBlocksCommitted();
            blocksExecuted = await mainContract.getTotalBlocksExecuted();
            tryCount += 1;
            await utils.sleep(1);
        }
        expect(blocksCommitted.gt(blocksExecuted), 'There is no committed but not executed block').to.be.true;
        blocksCommittedBeforeRevert = blocksCommitted;

        // Stop server.
        await killServerAndWaitForShutdown(tester);
    });

    step('revert blocks', async () => {
        const executedProcess = await utils.exec(
            'cd $ZKSYNC_HOME && ' +
                'RUST_LOG=off cargo run --bin block_reverter --release -- print-suggested-values --json'
            // ^ Switch off logs to not pollute the output JSON
        );
        const suggestedValuesOutput = executedProcess.stdout;
        const { lastL1BatchNumber, nonce, priorityFee } = parseSuggestedValues(suggestedValuesOutput);
        expect(lastL1BatchNumber < blocksCommittedBeforeRevert, 'There should be at least one block for revert').to.be
            .true;

        console.log(
            `Reverting with parameters: last unreverted L1 batch number: ${lastL1BatchNumber}, nonce: ${nonce}, priorityFee: ${priorityFee}`
        );

        console.log('Sending ETH transaction..');
        await utils.spawn(
            `cd $ZKSYNC_HOME && cargo run --bin block_reverter --release -- send-eth-transaction --l1-batch-number ${lastL1BatchNumber} --nonce ${nonce} --priority-fee-per-gas ${priorityFee}`
        );

        console.log('Rolling back DB..');
        await utils.spawn(
            `cd $ZKSYNC_HOME && cargo run --bin block_reverter --release -- rollback-db --l1-batch-number ${lastL1BatchNumber} --rollback-postgres --rollback-tree --rollback-sk-cache`
        );

        let blocksCommitted = await mainContract.getTotalBlocksCommitted();
        expect(blocksCommitted.eq(lastL1BatchNumber), 'Revert on contract was unsuccessful').to.be.true;
    });

    step('execute transaction after revert', async () => {
        // Set 1 second deadline for `ExecuteBlocks` operation.
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = '1';

        // Run server.
        utils.background('zk server --components api,tree,eth,data_fetcher,state_keeper', [null, logs, logs]);
        await utils.sleep(10);

        const balanceBefore = await alice.getBalance();
        expect(balanceBefore.eq(depositAmount.mul(2)), 'Incorrect balance after revert').to.be.true;

        // Execute a transaction
        const depositHandle = await tester.syncWallet.deposit({
            token: zkweb3.utils.ETH_ADDRESS,
            amount: depositAmount,
            to: alice.address
        });
        let receipt = await depositHandle.waitFinalize();
        expect(receipt.status).to.be.eql(1);

        const balanceAfter = await alice.getBalance();
        expect(balanceAfter.eq(BigNumber.from(depositAmount).mul(3)), 'Incorrect balance after another deposit').to.be
            .true;
    });

    step('execute transactions after simple restart', async () => {
        // Execute an L2 transaction
        await checkedRandomTransfer(alice, BigNumber.from(1));

        // Stop server.
        await killServerAndWaitForShutdown(tester);

        // Run again.
        utils.background(`zk server --components=api,tree,eth,data_fetcher,state_keeper`, [null, logs, logs]);
        await utils.sleep(10);

        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, BigNumber.from(1));
    });

    after('Try killing server', async () => {
        await utils.exec('pkill zksync_server').catch(ignoreError);
    });
});

async function checkedRandomTransfer(sender: zkweb3.Wallet, amount: BigNumber) {
    const senderBalanceBefore = await sender.getBalance();
    const receiver = zkweb3.Wallet.createRandom().connect(sender.provider);
    const transferHandle = await sender.sendTransaction({
        to: receiver.address,
        value: amount
    });
    const txReceipt = await transferHandle.wait();

    const senderBalance = await sender.getBalance();
    const receiverBalance = await receiver.getBalance();

    expect(receiverBalance.eq(amount), 'Failed updated the balance of the receiver').to.be.true;

    const spentAmount = txReceipt.gasUsed.mul(transferHandle.gasPrice!).add(amount);
    expect(senderBalance.add(spentAmount).eq(senderBalanceBefore), 'Failed to update the balance of the sender').to.be
        .true;
}
