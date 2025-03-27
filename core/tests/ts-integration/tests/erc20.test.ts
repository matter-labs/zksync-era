/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src';
import { Token } from '../src/types';
import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

// import * as zksync from 'zksync-ethers';
import * as zksync from 'zksync-ethers-interop-support';
import * as ethers from 'ethers';
import { scaledGasPrice, waitForL2ToL1LogProof } from '../src/helpers';
import { L2_DEFAULT_ETH_PER_ACCOUNT } from '../src/context-owner';

import {
    L2_MESSAGE_VERIFICATION_ADDRESS,
    ArtifactL2MessageVerification,
    ArtifactMessageRootStorage,
    L2_MESSAGE_ROOT_STORAGE_ADDRESS,
    ETH_ADDRESS_IN_CONTRACTS
} from '../src/constants';
import { RetryProvider } from '../src/retry-provider';

describe('L1 ERC20 contract checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
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
        const feeCheck = await shouldOnlyTakeFee(alice, true);
        await expect(
            alice.deposit({
                token: tokenDetails.l1Address,
                amount,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: {
                    gasPrice
                },
                overrides: {
                    gasPrice
                }
            })
        ).toBeAccepted([l1BalanceChange, l2BalanceChange, feeCheck]);
    });

    test('Can perform a transfer', async () => {
        const value = 200n;

        const balanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: -value },
            { wallet: bob, change: value }
        ]);
        const feeCheck = await shouldOnlyTakeFee(alice);

        // Send transfer, it should succeed.
        await expect(aliceErc20.transfer(bob.address, value)).toBeAccepted([balanceChange, feeCheck]);
    });

    test('Can perform a transfer to self', async () => {
        const value = 200n;

        // When transferring to self, balance should not change.
        const balanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [{ wallet: alice, change: 0n }]);
        const feeCheck = await shouldOnlyTakeFee(alice);
        await expect(aliceErc20.transfer(alice.address, value)).toBeAccepted([balanceChange, feeCheck]);
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
        const feeTaken = await shouldOnlyTakeFee(alice);

        // Send transfer, it should revert due to lack of balance.
        await expect(aliceErc20.transfer(bob.address, value, { gasLimit, gasPrice })).toBeReverted([
            noBalanceChange,
            feeTaken
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
        const feeTaken = await shouldOnlyTakeFee(alice);

        // Send transfer, it should revert because transfers to zero address are not allowed.
        await expect(aliceErc20.transfer(zeroAddress, value, { gasLimit, gasPrice })).toBeReverted([
            noBalanceChange,
            feeTaken
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

    let withdrawalHash: string;
    test('Can perform a withdrawal', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        const amount = 1n;

        const l2BalanceChange = await shouldChangeTokenBalances(tokenDetails.l2Address, [
            { wallet: alice, change: -amount }
        ]);
        const feeCheck = await shouldOnlyTakeFee(alice);
        const withdrawalPromise = alice.withdraw({
            token: tokenDetails.l2Address,
            amount
        });
        await expect(withdrawalPromise).toBeAccepted([l2BalanceChange, feeCheck]);
        const withdrawalTx = await withdrawalPromise;
        withdrawalHash = withdrawalTx.hash;
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

    test('Can check withdrawal hash in L2 ', async () => {
        // todo use the same chain, for simplicity.
        // For this we have to import proof until GW's message root, not until chain's chainBatchRoot.
        // this has to be allowed from the server.
        let interop2_provider = new RetryProvider({ url: 'http://localhost:3150', timeout: 1200 * 1000 }, undefined, testMaster.reporter)
        const l2MessageVerification = new zksync.Contract(
            L2_MESSAGE_VERIFICATION_ADDRESS,
            ArtifactL2MessageVerification.abi,
            interop2_provider
        );
        // console.log('l2MessageVerification', ArtifactL2MessageVerification.abi);
        const GATEWAY_CHAIN_ID = 506;
        const params = await alice.getFinalizeWithdrawalParams(withdrawalHash, undefined, undefined, GATEWAY_CHAIN_ID);
        console.log('withdrawalHash', withdrawalHash);
        let alice2Wallet = new zksync.Wallet(alice.privateKey, interop2_provider, testMaster.mainAccount().providerL1);
        // console.log(
        //     'cast call ',
        //     L2_MESSAGE_ROOT_STORAGE_ADDRESS,
        //     ' "msgRoots(uint256, uint256)" ',
        //     GATEWAY_CHAIN_ID,
        //     params.l1BatchNumber,
        //     ' -r localhost:3052'
        // );

        await delay(10000);
        await (
            await alice2Wallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                to: alice.address,
                amount: 1
            })
        ).wait();
        await delay(5000);

        // let message_root_storage = new zksync.Contract(
        //     L2_MESSAGE_ROOT_STORAGE_ADDRESS,
        //     ArtifactMessageRootStorage.abi,
        //     alice.provider
        // );
        // const msgRoots = await message_root_storage.msgRoots(
        //     GATEWAY_CHAIN_ID,
        //     params.l1BatchNumber
        // );
        // console.log('msgRoots', msgRoots);
        // if (msgRoots !== ethers.ZeroHash) {
        //     console.log('msgRoots', msgRoots);
        // }

        const included = await l2MessageVerification.proveL2MessageInclusionShared(
            (await alice.provider.getNetwork()).chainId,
            params.l1BatchNumber,
            params.l2MessageIndex,
            { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
            params.proof
        );
        // console.log(
        //     'l2MessageVerification',
        //     l2MessageVerification.interface.encodeFunctionData('proveL2MessageInclusionShared', [
        //         (await alice.provider.getNetwork()).chainId,
        //         params.l1BatchNumber,
        //         params.l2MessageIndex,
        //         { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
        //         params.proof
        //     ])
        // );
        // console.log(
        //     'cast call ',
        //     L2_MESSAGE_VERIFICATION_ADDRESS,
        //     ' "proveL2MessageInclusionShared(uint256,uint256,uint256,(uint16,address,bytes),bytes32[])" ',
        //     (await alice.provider.getNetwork()).chainId,
        //     params.l1BatchNumber,
        //     params.l2MessageIndex,
        //     { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
        //     params.proof,
        //     "-r localhost:3050"
        // );
        // console.log('included', included);
        expect(included).toBe(true);
    });

    function delay(ms: number) {
        return new Promise((resolve) => setTimeout(resolve, ms));
    }

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
            l2GasLimit: 5_000_000 // Setting the limit manually to avoid estimation for L1->L2 transaction
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

    test('Can perform a deposit with precalculated max value', async () => {
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
