import * as utils from 'zk/build/utils';
import { Tester } from './tester';
import * as zkweb3 from 'zksync-web3';
import { BigNumber, Contract, ethers, Wallet } from 'ethers';
import { expect } from 'chai';
import { hashBytecode } from 'zksync-web3/build/src/utils';
import fs from 'fs';
import path from 'path';
import { IZkSyncFactory } from 'zksync-web3/build/typechain';
import { TransactionResponse } from 'zksync-web3/build/src/types';

const depositAmount = ethers.utils.parseEther('0.001');

describe('Upgrade test', function () {
    let tester: Tester;
    let alice: zkweb3.Wallet;
    let mainContract: Contract;
    let bootloaderHash: string;

    before('create test wallet', async () => {
        tester = await Tester.init(process.env.CHAIN_ETH_NETWORK || 'localhost');
        alice = tester.emptyWallet();
    });

    step('run server and execute some transactions', async () => {
        // Make sure server isn't running.
        try {
            await utils.exec('pkill zksync_server');
            // It may take some time for witness generator to stop.
            await utils.sleep(120);
        } catch (_) {}

        // Set 1000 seconds deadline for `CommitBlock` operation.
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_COMMIT_DEADLINE = '1000';
        process.env.CHAIN_STATE_KEEPER_AGGREGATED_BLOCK_EXECUTE_DEADLINE = '1';
        // Run server in background.
        utils.background(
            `cd $ZKSYNC_HOME && cargo run --bin zksync_server --release -- --components=api,tree,eth,data_fetcher,state_keeper > server_logs.txt`
        );
        // Server may need some time to recompile if it's a cold run, so wait for it.
        let iter = 0;
        while (iter < 30 && !mainContract) {
            try {
                mainContract = await tester.syncWallet.getMainContract();
            } catch (_) {
                await utils.sleep(5);
            }
            iter += 1;
        }
        if (!mainContract) {
            throw new Error('Server did not start');
        }
        let blocksCommitted = await mainContract.getTotalBlocksCommitted();

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

        // Wait for at least one new committed block
        let newBlocksCommitted = await mainContract.getTotalBlocksCommitted();
        let tryCount = 0;
        while (blocksCommitted.eq(newBlocksCommitted) && tryCount < 10) {
            newBlocksCommitted = await mainContract.getTotalBlocksCommitted();
            tryCount += 1;
            await utils.sleep(1);
        }
    });

    step('Send l1 tx for saving new bootloader', async () => {
        const path = `${process.env.ZKSYNC_HOME}/etc/system-contracts/bootloader/build/artifacts/playground_block.yul/playground_block.yul.zbin`;
        const bootloaderCode = ethers.utils.hexlify(fs.readFileSync(path));
        bootloaderHash = ethers.utils.hexlify(hashBytecode(bootloaderCode));
        const txHandle = await tester.syncWallet.requestExecute({
            contractAddress: ethers.constants.AddressZero,
            calldata: '0x',
            l2GasLimit: 20000000,
            factoryDeps: [bootloaderCode],
            overrides: {
                gasLimit: 3000000
            }
        });
        await txHandle.wait();

        // Set the new bootloader hash and do not send the l1 batches with new bootloader
        process.env.CHAIN_STATE_KEEPER_BOOTLOADER_HASH = bootloaderHash;
        process.env.ETH_SENDER_SENDER_AGGREGATED_BLOCK_COMMIT_DEADLINE = '1';

        await utils.exec('pkill zksync_server');
        await utils.sleep(10);
        utils.background(
            `cd $ZKSYNC_HOME && cargo run --bin zksync_server --release -- --components=api,tree,eth,data_fetcher,state_keeper >> server_logs.txt`
        );
        await utils.sleep(10);
        // Wait for finalizing the last tx with old bootloader
        await txHandle.waitFinalize();
        // Create one more tx with the new bootloader
        await checkedRandomTransfer(alice, BigNumber.from(1));
    });

    step('upgrade bootloader on contract', async () => {
        const testConfigPath = path.join(process.env.ZKSYNC_HOME as string, `etc/test_config/constant`);
        const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
        const deployWallet = Wallet.fromMnemonic(ethTestConfig.mnemonic, "m/44'/60'/0'/0/1").connect(
            tester.ethProvider
        );

        const address = await tester.web3Provider.getMainContractAddress();
        const contract = IZkSyncFactory.connect(address, deployWallet);
        let tx = await contract.setL2BootloaderBytecodeHash(bootloaderHash);
        await tx.wait(10);
        // Restart server. And start sending the blocks with the new bootloader
        await utils.exec('pkill zksync_server');
        await utils.sleep(10);
        utils.background(
            `cd $ZKSYNC_HOME && cargo run --bin zksync_server --release -- --components=api,tree,eth,data_fetcher,state_keeper >> server_logs.txt`
        );
        await utils.sleep(10);
    });

    step('execute transactions after simple restart', async () => {
        // Execute an L2 transaction
        const txHandle = await checkedRandomTransfer(alice, BigNumber.from(1));
        await txHandle.waitFinalize();

        // Stop server.
        await utils.exec('pkill zksync_server');
        await utils.sleep(10);

        // Run again.
        utils.background(
            `cd $ZKSYNC_HOME && cargo run --bin zksync_server --release -- --components=api,tree,eth,data_fetcher,state_keeper >> server_logs.txt`
        );
        await utils.sleep(10);

        // Trying to send a transaction from the same address again
        await checkedRandomTransfer(alice, BigNumber.from(1));

        let bootloaderHashL1 = await mainContract.getL2BootloaderBytecodeHash();
        expect(bootloaderHashL1).eq(bootloaderHash);
    });

    after('Try killing server', async () => {
        try {
            await utils.exec('pkill zksync_server');
        } catch (_) {}
    });
});

async function checkedRandomTransfer(sender: zkweb3.Wallet, amount: BigNumber): Promise<TransactionResponse> {
    const senderBalanceBefore = await sender.getBalance();
    const receiver = zkweb3.Wallet.createRandom().connect(sender.provider);
    const transferHandle = await sender.sendTransaction({
        to: receiver.address,
        value: amount
    });
    const txReceipt = await transferHandle.wait();

    const senderBalanceAfter = await sender.getBalance();
    const receiverBalanceAfter = await receiver.getBalance();

    expect(receiverBalanceAfter.eq(amount), 'Failed updated the balance of the receiver').to.be.true;

    const spentAmount = txReceipt.gasUsed.mul(transferHandle.gasPrice!).add(amount);
    expect(senderBalanceAfter.add(spentAmount).eq(senderBalanceBefore), 'Failed to update the balance of the sender').to
        .be.true;
    return transferHandle;
}
