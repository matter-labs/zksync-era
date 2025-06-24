/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src';
import { Token } from '../src/types';
import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { scaledGasPrice, waitForL2ToL1LogProof } from '../src/helpers';
import { L2_DEFAULT_ETH_PER_ACCOUNT } from '../src/context-owner';
import { RetryableWallet } from '../src/retry-provider';
import { log } from 'console';
import { isAddressEq, L1_MESSENGER_ADDRESS } from 'zksync-ethers/build/utils';

describe('L1 ERC20 contract checks', () => {
    let testMaster: TestMaster;
    let alice: RetryableWallet;
    let bob: zksync.Wallet;
    let isETHBasedChain: boolean;
    let baseTokenAddress: string;
    let tokenDetails: Token;
    let aliceErc20: zksync.Contract;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();

        // Get the information about base token address directly from the L2.
        baseTokenAddress = await alice._providerL2().getBaseTokenContractAddress();
        isETHBasedChain = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;

        tokenDetails = testMaster.environment().erc20Token;
        aliceErc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, alice);
    });

    test('Token properties are correct', async () => {
        // const mainContract = await alice.getMainContract();
        // console.log("Address : ",  await mainContract.getAddress());
        // const res = await mainContract.proveL2MessageInclusion(
        //     94,
        //     0,
        //     {
        //         txNumberInBatch: 1,
        //         sender: '0x0000000000000000000000000000000000010003',
        //         data: '0x9c884fd1000000000000000000000000000000000000000000000000000000000000010fe64fc3dce176ccd51cd2b3d1bf55627aadc066d41c11825a6946c0f066072230000000000000000000000000da6619b909edb6f811ff9a8c21ec8ba3082acee6000000000000000000000000da6619b909edb6f811ff9a8c21ec8ba3082acee6000000000000000000000000b8d194ecbbbef524e416ca62f13b5bf461158ee2000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001c1010000000000000000000000000000000000000000000000000000000000000009000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000180000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000004574254430000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000457425443000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000'
        //     },
        //     [
        //         '0x010f000100000000000000000000000000000000000000000000000000000000',
        //         '0x72abee45b59e344af8a6e520241c4744aff26ed411f4c4b00f8af09adada43ba',
        //         '0xc3d03eebfd83049991ea3d3e358b6712e7aa2e2e63dc2d4b438987cec28ac8d0',
        //         '0xe3697c7f33c31a9b0f0aeb8542287d0d21e8c4cf82163d0c44c7a98aa11aa111',
        //         '0x199cc5812543ddceeddd0fc82807646a4899444240db2c0d2f20c3cceb5f51fa',
        //         '0xe4733f281f18ba3ea8775dd62d2fcd84011c8c938f16ea5790fd29a03bf8db89',
        //         '0x1798a1fd9c8fbb818c98cff190daa7cc10b6e5ac9716b4a2649f7c2ebcef2272',
        //         '0x66d7c5983afe44cf15ea8cf565b34c6c31ff0cb4dd744524f7842b942d08770d',
        //         '0xb04e5ee349086985f74b73971ce9dfe76bbed95c84906c5dffd96504e1e5396c',
        //         '0xac506ecb5465659b3a927143f6d724f91d8d9c4bdb2463aee111d9aa869874db',
        //         '0x124b05ec272cecd7538fdafe53b6628d31188ffb6f345139aac3c3c1fd2e470f',
        //         '0xc3be9cbd19304d84cca3d045e06b8db3acd68c304fc9cd4cbffe6d18036cb13f',
        //         '0xfef7bd9f889811e59e4076a0174087135f080177302763019adaf531257e3a87',
        //         '0xa707d1c62d8be699d34cb74804fdd7b4c568b6c1a821066f126c680d4b83e00b',
        //         '0xf6e093070e0389d2e529d60fadb855fdded54976ec50ac709e3a36ceaa64c291',
        //         '0x0000000000000000000000000000000000000000000000000000000000000000'
        //     ]
        // );
        // console.log("res : ", res);
        await expect(aliceErc20.name()).resolves.toBe(tokenDetails.name);
        await expect(aliceErc20.decimals()).resolves.toBe(tokenDetails.decimals);
        await expect(aliceErc20.symbol()).resolves.toBe(tokenDetails.symbol);
        await expect(aliceErc20.balanceOf(alice.address)).resolves.toBeGreaterThan(0n); // 'Alice should have non-zero balance'
    });

    test('Can perform a deposit', async () => {
        const amount = 1n; // 1 wei is enough.
        const gasPrice = await scaledGasPrice(alice);

        // Note: for L1 we should use L1 token address.
        const l1BalanceChange = await shouldChangeTokenBalances(
            tokenDetails.l1Address,
            [{ wallet: alice, change: -amount }],
            {
                l1: true
            }
        );
        const l2BalanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: amount }
        ]);
        // FIXME: include fee check, it has been temporarily removed, probably due to issues with refunds in L1->L2 txs.
        // const feeCheck = await shouldOnlyTakeFee(alice, true);
        await alice.retryableDepositCheck(
            {
                token: tokenDetails.l1Address,
                amount,
                approveERC20: true,
                approveBaseERC20: true,
                // FIXME: gas estimation does not work for L1->L2 txs
                l2GasLimit: 1000000,
                approveOverrides: {
                    gasPrice
                },
                overrides: {
                    gasPrice
                }
            },
            (deposit) => expect(deposit).toBeAccepted([l1BalanceChange, l2BalanceChange])
        );
    });

    test('Can perform a transfer', async () => {
        const value = 200n;

        const balanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: -value },
            { wallet: bob, change: value }
        ]);
        // FIXME: include fee check, it has been temporarily removed, probably due to issues with refunds in L1->L2 txs.
        // const feeCheck = await shouldOnlyTakeFee(alice);

        // Send transfer, it should succeed.
        await expect(aliceErc20.transfer(bob.address, value)).toBeAccepted([balanceChange]);
    });

    test('Can perform a transfer to self', async () => {
        const value = 200n;

        // When transferring to self, balance should not change.
        const balanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [{ wallet: alice, change: 0n }]);
        // FIXME: include fee check, it has been temporarily removed, probably due to issues with refunds in L1->L2 txs.
        // const feeCheck = await shouldOnlyTakeFee(alice);
        await expect(aliceErc20.transfer(alice.address, value)).toBeAccepted([balanceChange]);
    });

    test('Incorrect transfer should revert', async () => {
        const value = ethers.parseEther('1000000.0');
        const gasPrice = await scaledGasPrice(alice);

        // Since gas estimation is expected to fail, we request gas limit for similar non-failing tx.
        const gasLimit = await aliceErc20.transfer.estimateGas(bob.address, 1);

        // Balances should not change for this token.
        const noBalanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: 0n },
            { wallet: bob, change: 0n }
        ]);
        // Fee in ETH should be taken though.
        // FIXME: include fee check, it has been temporarily removed, probably due to issues with refunds in L1->L2 txs.
        // const feeTaken = await shouldOnlyTakeFee(alice);

        // Send transfer, it should revert due to lack of balance.
        await expect(aliceErc20.transfer(bob.address, value, { gasLimit, gasPrice })).toBeReverted([
            noBalanceChange
            // feeTaken
        ]);
    });

    test('Transfer to zero address should revert', async () => {
        const zeroAddress = ethers.ZeroAddress;
        const value = 200n;
        const gasPrice = await scaledGasPrice(alice);

        // Since gas estimation is expected to fail, we request gas limit for similar non-failing tx.
        const gasLimit = await aliceErc20.transfer.estimateGas(bob.address, 1);

        // Balances should not change for this token.
        const noBalanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: 0n }
        ]);
        // Fee in ETH should be taken though.
        // FIXME: include fee check, it has been temporarily removed, probably due to issues with refunds in L1->L2 txs.
        // const feeTaken = await shouldOnlyTakeFee(alice);

        // Send transfer, it should revert because transfers to zero address are not allowed.
        await expect(aliceErc20.transfer(zeroAddress, value, { gasLimit, gasPrice })).toBeReverted([
            noBalanceChange
            // feeTaken
        ]);
    });

    test('Approve and transferFrom should work', async () => {
        const approveAmount = 42n;
        const bobErc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, bob);

        // Fund bob's account to perform a transaction from it.
        await alice
            .transfer({
                to: bob.address,
                amount: L2_DEFAULT_ETH_PER_ACCOUNT / 8n,
                token: zksync.utils.L2_BASE_TOKEN_ADDRESS
            })
            .then((tx) => tx.wait());

        await expect(aliceErc20.allowance(alice.address, bob.address)).resolves.toEqual(0n);
        await expect(aliceErc20.approve(bob.address, approveAmount)).toBeAccepted();
        await expect(aliceErc20.allowance(alice.address, bob.address)).resolves.toEqual(approveAmount);
        await expect(bobErc20.transferFrom(alice.address, bob.address, approveAmount)).toBeAccepted();
        await expect(aliceErc20.allowance(alice.address, bob.address)).resolves.toEqual(0n);
    });

    test('Can perform a withdrawal', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        const amount = 1n;

        const l2BalanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: -amount }
        ]);
        // TODO: include fee check
        // const feeCheck = await shouldOnlyTakeFee(alice);
        const withdrawalPromise = alice.withdraw({
            token: tokenDetails.l2Address,
            amount
        });
        await expect(withdrawalPromise).toBeAccepted([l2BalanceChange]);
        const withdrawalTx = await withdrawalPromise;
        const l2TxReceipt = await alice.provider.getTransactionReceipt(withdrawalTx.hash);
        await waitForL2ToL1LogProof(alice, l2TxReceipt!.blockNumber, withdrawalTx.hash);

        // Note: For L1 we should use L1 token address.
        const l1BalanceChange = await shouldChangeTokenBalances(
            tokenDetails.l1Address,
            [{ wallet: alice, change: amount }],
            {
                l1: true
            }
        );

        await expect(alice.finalizeWithdrawal(withdrawalTx.hash)).toBeAccepted([l1BalanceChange]);
    });

    test('Should claim failed deposit', async () => {
        if (testMaster.isFastMode()) {
            return;
        }

        const amount = 1n;
        const initialBalance = await alice.getBalanceL1(tokenDetails.l1Address);
        // Deposit to the zero address is forbidden and should fail with the current implementation.
        const depositHandle = await alice.deposit({
            token: tokenDetails.l1Address,
            to: ethers.ZeroAddress,
            amount,
            approveERC20: true,
            approveBaseERC20: true,
            l2GasLimit: 500_000 // Setting the limit manually to avoid estimation for L1->L2 transaction
        });
        const l1Receipt = await depositHandle.waitL1Commit();

        // L1 balance should change, but tx should fail in L2.
        await expect(alice.getBalanceL1(tokenDetails.l1Address)).resolves.toEqual(initialBalance - amount);
        await expect(depositHandle).toBeReverted();

        // Wait for tx to be finalized.
        // `waitFinalize` is not used because it doesn't work as expected for failed transactions.
        // It throws once it gets status == 0 in the receipt and doesn't wait for the finalization.
        const l2Hash = zksync.utils.getL2HashFromPriorityOp(l1Receipt, await alice.provider.getMainContractAddress());
        const l2TxReceipt = await alice.provider.getTransactionReceipt(l2Hash);
        await waitForL2ToL1LogProof(alice, l2TxReceipt!.blockNumber, l2Hash);
        // Claim failed deposit.
        await expect(alice.claimFailedDeposit(l2Hash)).toBeAccepted();
        await expect(alice.getBalanceL1(tokenDetails.l1Address)).resolves.toEqual(initialBalance);
    });

    test.skip('Can perform a deposit with precalculated max value', async () => {
        if (!isETHBasedChain) {
            // approving whole base token balance
            const baseTokenDetails = testMaster.environment().baseToken;
            const baseTokenMaxAmount = await alice.getBalanceL1(baseTokenDetails.l1Address);
            await (await alice.approveERC20(baseTokenDetails.l1Address, baseTokenMaxAmount)).wait();
        }

        // depositing the max amount: the whole balance of the token
        const tokenDepositAmount = await alice.getBalanceL1(tokenDetails.l1Address);

        // approving the needed allowance for the deposit
        await (await alice.approveERC20(tokenDetails.l1Address, tokenDepositAmount)).wait();

        // fee of the deposit in ether
        const depositFee = await alice.getFullRequiredDepositFee({
            token: tokenDetails.l1Address
        });

        // checking if alice has enough funds to pay the fee
        const l1Fee = depositFee.l1GasLimit * (depositFee.maxFeePerGas! || depositFee.gasPrice!);
        const l2Fee = depositFee.baseCost;
        const aliceBalance = await alice.getBalanceL1();
        if (aliceBalance < l1Fee + l2Fee) {
            throw new Error('Not enough balance to pay the fee');
        }

        // deposit handle with the precalculated max amount
        const depositHandle = await alice.deposit({
            token: tokenDetails.l1Address,
            amount: tokenDepositAmount,
            l2GasLimit: depositFee.l2GasLimit,
            approveBaseERC20: true,
            approveERC20: true,
            overrides: depositFee
        });

        // checking the l2 balance change
        const l2TokenBalanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: tokenDepositAmount }
        ]);
        await expect(depositHandle).toBeAccepted([l2TokenBalanceChange]);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
