/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src';
import { Token } from '../src/types';
import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { Provider, Wallet } from 'ethers';
import { scaledGasPrice, deployContract, readContract, waitForBlockToBeFinalizedOnL1 } from '../src/helpers';

describe('L2 native ERC20 contract checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let isETHBasedChain: boolean;
    let baseTokenAddress: string;
    let zkTokenAssetId: string;
    let tokenDetails: Token;
    let aliceErc20: zksync.Contract;
    let l1NativeTokenVault: ethers.Contract;
    let l1Wallet: Wallet;
    let l2Wallet: Wallet;
    let l1Provider: Provider;
    let l2Provider: Provider;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        const bridgeContracts = await alice.getL1BridgeContracts();
        const assetRouter = bridgeContracts.shared;
        l2Provider = alice._providerL2();
        l1Provider = alice._providerL1();
        l2Wallet = new Wallet(alice.privateKey, l2Provider);
        l1Wallet = new Wallet(alice.privateKey, l1Provider);
        const L2_NATIVE_TOKEN_VAULT_ADDRESS = '0x0000000000000000000000000000000000010004';
        const ARTIFACTS_PATH = '../../../contracts/l1-contracts/out';
        const l2NtvInterface = readContract(`${ARTIFACTS_PATH}`, 'L2NativeTokenVault').abi;
        const l2NativeTokenVault = new ethers.Contract(L2_NATIVE_TOKEN_VAULT_ADDRESS, l2NtvInterface, l2Wallet);
        const l1AssetRouterInterface = readContract(`${ARTIFACTS_PATH}`, 'L1AssetRouter').abi;
        const l1NativeTokenVaultInterface = readContract(`${ARTIFACTS_PATH}`, 'L1NativeTokenVault').abi;
        const l1AssetRouter = new ethers.Contract(await assetRouter.getAddress(), l1AssetRouterInterface, l1Wallet);
        l1NativeTokenVault = new ethers.Contract(
            await l1AssetRouter.nativeTokenVault(),
            l1NativeTokenVaultInterface,
            l1Wallet
        );

        // Get the information about base token address directly from the L2.
        baseTokenAddress = await alice._providerL2().getBaseTokenContractAddress();
        isETHBasedChain = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;

        const ZkSyncERC20 = await readContract(
            '../../../contracts/l1-contracts/zkout',
            'TestnetERC20Token'
        );

        aliceErc20 = await deployContract(alice, ZkSyncERC20, ['ZKsync', 'ZK', 18]);
        const l2TokenAddress = await aliceErc20.getAddress();
        tokenDetails = {
            name: 'ZKsync',
            symbol: 'ZK',
            decimals: 18n,
            l1Address: ethers.ZeroAddress,
            l2Address: l2TokenAddress
        };
        const mintTx = await aliceErc20.mint(alice.address, 1000n);
        await mintTx.wait();
        // const mintTx2 = await aliceErc20.mint('0x36615Cf349d7F6344891B1e7CA7C72883F5dc049', 1000n);
        // await mintTx2.wait();
        const registerZKTx = await l2NativeTokenVault.registerToken(tokenDetails.l2Address);
        await registerZKTx.wait();
        zkTokenAssetId = await l2NativeTokenVault.assetId(l2TokenAddress);
        const tokenApprovalTx = await aliceErc20.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, 100n);
        await tokenApprovalTx.wait();
    });

    test('Token properties are correct', async () => {
        await expect(aliceErc20.name()).resolves.toBe(tokenDetails.name);
        await expect(aliceErc20.decimals()).resolves.toBe(tokenDetails.decimals);
        await expect(aliceErc20.symbol()).resolves.toBe(tokenDetails.symbol);
        await expect(aliceErc20.balanceOf(alice.address)).resolves.toBeGreaterThan(0n); // 'Alice should have non-zero balance'
    });

    test('Can perform a withdrawal', async () => {
        if (testMaster.isFastMode()) {
            return;
        }
        const amount = 10n;

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
        const l2TxReceipt = await alice.provider.getTransactionReceipt(withdrawalTx.hash);
        await withdrawalTx.waitFinalize();
        await waitForBlockToBeFinalizedOnL1(alice, l2TxReceipt!.blockNumber);

        await alice.finalizeWithdrawalParams(withdrawalTx.hash); // kl todo finalize the Withdrawals with the params here. Alternatively do in the SDK.
        await expect(alice.finalizeWithdrawal(withdrawalTx.hash)).toBeAccepted();

        tokenDetails.l1Address = await l1NativeTokenVault.tokenAddress(zkTokenAssetId);
        const balanceAfterBridging = await alice.getBalanceL1(tokenDetails.l1Address);
        expect(balanceAfterBridging).toEqual(10n);
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
        await waitForBlockToBeFinalizedOnL1(alice, l2TxReceipt!.blockNumber);
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
