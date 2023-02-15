import * as utils from 'zk/build/utils';
import { Tester } from './tester';
import * as zkweb3 from 'zksync-web3';
import { BigNumber, Contract, ethers } from 'ethers';
import { expect } from 'chai';

// Parses output of "print-suggested-values" command of the revert block tool.
function parseSuggestedValues(suggestedValuesString: string) {
    let result = suggestedValuesString.match(/(?<=l1 batch number: |nonce: |priority fee: )[0-9]*/g)!;
    return { lastL1BatchNumber: parseInt(result[0]), nonce: parseInt(result[1]), priorityFee: parseInt(result[2]) };
}

async function killServerAndWaitForShutdown(tester: Tester) {
    await utils.exec('pkill zksync_server');
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

const depositAmount = ethers.utils.parseEther('0.001');

describe('Block reverting test', function () {
    let tester: Tester;
    let alice: zkweb3.Wallet;
    let mainContract: Contract;
    let blocksCommittedBeforeRevert: number;

    before('create test wallet', async () => {
        tester = await Tester.init(process.env.CHAIN_ETH_NETWORK || 'localhost');
        alice = tester.emptyWallet();
    });

    step('run server and execute some transactions', async () => {
        // Make sure server isn't running.
        try {
            await killServerAndWaitForShutdown(tester);
        } catch (_) {}

        // Set 1000 seconds deadline for `ExecuteBlocks` operation.
        process.env.CHAIN_STATE_KEEPER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = '1000';

        // Run server in background.
        utils.background(`zk server --components api,tree,tree_lightweight,eth,data_fetcher,state_keeper`);
        // Server may need some time to recompile if it's a cold run, so wait for it.
        let iter = 0;
        while (iter < 30 && !mainContract) {
            try {
                mainContract = await tester.syncWallet.getMainContract();
            } catch (_) {
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
        let suggestedValuesOutput = (
            await utils.exec(`cd $ZKSYNC_HOME && cargo run --bin block_reverter --release -- print-suggested-values`)
        ).stdout;
        let { lastL1BatchNumber, nonce, priorityFee } = parseSuggestedValues(suggestedValuesOutput);
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
        process.env.CHAIN_STATE_KEEPER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = '1';

        // Run server.
        utils.background(`zk server --components api,tree,tree_lightweight,eth,data_fetcher,state_keeper`);
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
        utils.background(`zk server --components=api,tree,tree_lightweight,eth,data_fetcher,state_keeper`);
        await utils.sleep(10);

        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, BigNumber.from(1));
    });

    after('Try killing server', async () => {
        try {
            await utils.exec('pkill zksync_server');
        } catch (_) {}
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
