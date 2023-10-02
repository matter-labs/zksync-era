/**
 * This suite contains tests checking the interaction with L2 WETH Bridge/Token.
 */
import { TestMaster } from '../src/index';

import * as zksync from 'zksync-web3';
import { scaledGasPrice, waitUntilBlockFinalized } from '../src/helpers';
import { WETH9, WETH9Factory } from 'l1-zksync-contracts/typechain';
import { L2Weth, L2WethFactory } from 'l2-zksync-contracts/typechain';
import { BigNumber, ethers } from 'ethers';
import {
    shouldChangeETHBalances,
    shouldChangeTokenBalances,
    shouldOnlyTakeFee
} from '../src/modifiers/balance-checker';
import { L2_ETH_PER_ACCOUNT } from '../src/context-owner';

describe('Tests for the WETH bridge/token behavior', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let aliceL1Weth: WETH9;
    let aliceL2Weth: L2Weth;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();

        const l1WethTokenAddress = testMaster.environment().wethToken.l1Address;
        aliceL1Weth = WETH9Factory.connect(l1WethTokenAddress, alice._signerL1());

        const l2WethTokenAddress = testMaster.environment().wethToken.l2Address;
        aliceL2Weth = L2WethFactory.connect(l2WethTokenAddress, alice._signerL2());
    });

    test('Should deposit WETH', async () => {
        let balance = await alice.getBalanceL1();
        let transferTx = await alice._signerL1().sendTransaction({
            to: bob.address,
            value: balance.div(2)
        });
        await transferTx.wait();

        const gasPrice = await scaledGasPrice(alice);
        // Convert Ether to WETH.
        const amount = 1000; // 1000 wei is enough
        await (await aliceL1Weth.deposit({ value: amount, gasPrice })).wait();

        const initialBalanceL1 = await alice.getBalanceL1(aliceL1Weth.address);
        const initialBalanceL2 = await alice.getBalance(aliceL2Weth.address);
        let tx = await alice.deposit({
            token: aliceL1Weth.address,
            amount,
            approveERC20: true,
            approveOverrides: {
                gasPrice
            },
            overrides: {
                gasPrice
            }
        });

        await tx.wait();
        await expect(alice.getBalanceL1(aliceL1Weth.address)).resolves.bnToBeEq(initialBalanceL1.sub(amount));
        await expect(alice.getBalance(aliceL2Weth.address)).resolves.bnToBeEq(initialBalanceL2.add(amount));
    });

    test('Should transfer WETH', async () => {
        const value = BigNumber.from(200);

        const balanceChange = await shouldChangeTokenBalances(aliceL2Weth.address, [
            { wallet: alice, change: -value },
            { wallet: bob, change: value }
        ]);
        const feeCheck = await shouldOnlyTakeFee(alice);

        // Send transfer, it should succeed.
        await expect(aliceL2Weth.transfer(bob.address, value)).toBeAccepted([balanceChange, feeCheck]);
    });

    test('Can unwrap WETH on L2', async () => {
        const value = BigNumber.from(200);

        const tokenBalanceChange = await shouldChangeTokenBalances(aliceL2Weth.address, [
            { wallet: alice, change: -value }
        ]);
        const ethBalanceChange = await shouldChangeETHBalances([{ wallet: alice, change: value }]);
        await expect(aliceL2Weth.withdraw(value)).toBeAccepted([tokenBalanceChange, ethBalanceChange]);
    });

    test('Approve and transferFrom should work', async () => {
        const approveAmount = 42;
        const bobErc20 = aliceL2Weth.connect(bob);

        // Fund bob's account to perform a transaction from it.
        await alice
            .transfer({ to: bob.address, amount: L2_ETH_PER_ACCOUNT.div(8), token: zksync.utils.ETH_ADDRESS })
            .then((tx) => tx.wait());

        const bobTokenBalanceChange = await shouldChangeTokenBalances(aliceL2Weth.address, [
            { wallet: alice, change: -approveAmount },
            { wallet: bob, change: approveAmount }
        ]);

        await expect(aliceL2Weth.allowance(alice.address, bob.address)).resolves.bnToBeEq(0);
        await expect(aliceL2Weth.approve(bob.address, approveAmount)).toBeAccepted();
        await expect(aliceL2Weth.allowance(alice.address, bob.address)).resolves.bnToBeEq(approveAmount);
        await expect(bobErc20.transferFrom(alice.address, bob.address, approveAmount)).toBeAccepted([
            bobTokenBalanceChange
        ]);
        await expect(aliceL2Weth.allowance(alice.address, bob.address)).resolves.bnToBeEq(0);
    });

    test('Can perform a withdrawal', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        const amount = 1;

        const l2BalanceChange = await shouldChangeTokenBalances(aliceL2Weth.address, [
            { wallet: alice, change: -amount }
        ]);
        const feeCheck = await shouldOnlyTakeFee(alice);
        const withdrawalPromise = alice.withdraw({ token: aliceL2Weth.address, amount });
        await expect(withdrawalPromise).toBeAccepted([l2BalanceChange, feeCheck]);
        const withdrawalTx = await withdrawalPromise;
        await withdrawalTx.waitFinalize();

        // Note: For L1 we should use L1 token address.
        const l1BalanceChange = await shouldChangeTokenBalances(
            aliceL1Weth.address,
            [{ wallet: alice, change: amount }],
            {
                l1: true
            }
        );
        await expect(alice.finalizeWithdrawal(withdrawalTx.hash)).toBeAccepted([l1BalanceChange]);
    });

    test('Should fail to claim failed deposit', async () => {
        if (testMaster.isFastMode()) {
            return;
        }

        const amount = 1;
        const initialWethL1Balance = await alice.getBalanceL1(aliceL1Weth.address);
        const initialWethL2Balance = await alice.getBalance(aliceL2Weth.address);
        const initialEthL2Balance = await alice.getBalance();
        // Deposit to the zero address is forbidden and should fail with the current implementation.
        const depositHandle = await alice.deposit({
            to: ethers.constants.AddressZero,
            token: aliceL1Weth.address,
            amount,
            l2GasLimit: 5_000_000, // Setting the limit manually to avoid estimation for L1->L2 transaction
            approveERC20: true
        });
        const l1Receipt = await depositHandle.waitL1Commit();

        // L1 balance should change, but tx should fail in L2.
        await expect(alice.getBalanceL1(aliceL1Weth.address)).resolves.bnToBeEq(initialWethL1Balance.sub(amount));
        await expect(depositHandle).toBeReverted();

        // Wait for tx to be finalized.
        // `waitFinalize` is not used because it doesn't work as expected for failed transactions.
        // It throws once it gets status == 0 in the receipt and doesn't wait for the finalization.
        const l2Hash = zksync.utils.getL2HashFromPriorityOp(l1Receipt, await alice.provider.getMainContractAddress());
        const l2TxReceipt = await alice.provider.getTransactionReceipt(l2Hash);
        await waitUntilBlockFinalized(alice, l2TxReceipt.blockNumber);

        // Try to claim failed deposit, which should revert, and ETH should be returned on L2.
        await expect(alice.claimFailedDeposit(l2Hash)).toBeRevertedEstimateGas();
        await expect(alice.getBalanceL1(aliceL1Weth.address)).resolves.bnToBeEq(initialWethL1Balance.sub(amount));
        await expect(alice.getBalance(aliceL2Weth.address)).resolves.bnToBeEq(initialWethL2Balance);
        await expect(alice.getBalance()).resolves.bnToBeGte(initialEthL2Balance.add(amount));
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
