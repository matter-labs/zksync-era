/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src';
import { Token } from '../src/types';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { scaledGasPrice, waitForL2ToL1LogProof } from '../src/helpers';
import { IL2NativeTokenVault__factory } from 'zksync-ethers/build/typechain';
import { RetryableWallet } from '../src/retry-provider';

const SECONDS = 2000;
jest.setTimeout(100 * SECONDS);

describe('base ERC20 contract checks', () => {
    let testMaster: TestMaster;
    let alice: RetryableWallet;
    let bob: zksync.Wallet;
    let baseTokenDetails: Token;
    let isETHBasedChain: boolean;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();

        baseTokenDetails = testMaster.environment().baseToken;
        const baseToken = await alice.provider.getBaseTokenContractAddress();
        isETHBasedChain = zksync.utils.isAddressEq(baseToken, zksync.utils.ETH_ADDRESS_IN_CONTRACTS);
    });

    test('Base token ratio is updated on L1', async () => {
        if (isETHBasedChain) {
            return;
        }

        const zksyncAddress = await alice._providerL2().getMainContractAddress();
        const zksyncContract = new ethers.Contract(zksyncAddress, zksync.utils.ZKSYNC_MAIN_ABI, alice.ethWallet());
        const numerator = Number(await zksyncContract.baseTokenGasPriceMultiplierNominator());
        const denominator = Number(await zksyncContract.baseTokenGasPriceMultiplierDenominator());

        expect(numerator).toBe(314);
        expect(denominator).toBe(1000);
    });

    test('Can perform a deposit', async () => {
        const amount = 1n; // 1 wei is enough.
        const gasPrice = await scaledGasPrice(alice);

        const initialEthBalance = await alice.getBalanceL1();
        const initialL1Balance = await alice.getBalanceL1(baseTokenDetails.l1Address);
        const initialL2Balance = await alice.getBalance();

        const depositHash = await alice.retryableDepositCheck(
            {
                token: baseTokenDetails.l1Address,
                amount: amount,
                approveERC20: true,
                approveBaseERC20: true,
                approveBaseOverrides: {
                    gasPrice
                },
                approveOverrides: {
                    gasPrice
                },
                overrides: {
                    gasPrice
                }
            },
            async (deposit) => {
                await deposit.wait();
                return deposit.hash;
            }
        );

        const receipt = await alice._providerL1().getTransactionReceipt(depositHash);
        if (!receipt) {
            throw new Error('No receipt for deposit');
        }
        const fee = receipt.gasPrice * receipt.gasUsed;

        // TODO: should all the following tests use strict equality?

        const finalEthBalance = await alice.getBalanceL1();
        expect(initialEthBalance).toBeGreaterThanOrEqual(finalEthBalance + fee); // Fee should be taken from the ETH balance on L1.

        const finalL1Balance = await alice.getBalanceL1(baseTokenDetails.l1Address);
        expect(initialL1Balance).toBeGreaterThanOrEqual(finalL1Balance + amount);

        const finalL2Balance = await alice.getBalance();
        expect(initialL2Balance).toBeLessThanOrEqual(finalL2Balance + amount);
    });

    test('Not enough balance should revert', async () => {
        const amount = BigInt('0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffff');
        const gasPrice = await scaledGasPrice(alice);
        let errorMessage;

        await expect(
            alice.deposit({
                token: baseTokenDetails.l1Address,
                amount: amount,
                approveERC20: true,
                approveBaseERC20: true,
                approveBaseOverrides: {
                    gasPrice
                },
                approveOverrides: {
                    gasPrice
                },
                overrides: {
                    gasPrice
                }
            })
        ).toBeRejected(errorMessage);
    });

    test('Can perform a transfer to self', async () => {
        const amount = 200n;

        const initialAliceBalance = await alice.getBalance();

        // When transferring to self, balance should only change by fee from tx.
        const transferPromise = alice.transfer({
            to: alice.address,
            amount
        });

        await expect(transferPromise).toBeAccepted([]);
        const transferTx = await transferPromise;
        await transferTx.waitFinalize();

        const receipt = await alice._providerL2().getTransactionReceipt(transferTx.hash);
        const fee = receipt!.gasPrice * receipt!.gasUsed;

        const finalAliceBalance = await alice.getBalance();
        expect(initialAliceBalance - fee).toEqual(finalAliceBalance);
    });

    test('Incorrect transfer should revert', async () => {
        const amount = ethers.parseEther('1000000.0');

        const initialAliceBalance = await alice.getBalance();
        const initialBobBalance = await bob.getBalance();

        // Send transfer, it should reject due to lack of balance.
        await expect(
            alice.transfer({
                to: bob.address,
                amount
            })
        ).toBeRejected();

        // Balances should not change for this token.
        const finalAliceBalance = await alice.getBalance();
        const finalBobBalance = await bob.getBalance();

        await expect(finalAliceBalance).toEqual(initialAliceBalance);
        await expect(finalBobBalance).toEqual(initialBobBalance);
    });

    test('Can perform a withdrawal', async () => {
        if (testMaster.isFastMode() || isETHBasedChain) {
            return;
        }
        const amount = 1n;

        const initialL1Balance = await alice.getBalanceL1(baseTokenDetails.l1Address);
        const initialL2Balance = await alice.getBalance();

        const withdrawalPromise = alice.withdraw({ token: baseTokenDetails.l2Address, amount });
        await expect(withdrawalPromise).toBeAccepted([]);
        const withdrawalTx = await withdrawalPromise;
        const l2Receipt = await withdrawalTx.wait();
        await waitForL2ToL1LogProof(alice, l2Receipt!.blockNumber, withdrawalTx.hash);

        await expect(alice.finalizeWithdrawal(withdrawalTx.hash)).toBeAccepted([]);
        const receipt = await alice._providerL2().getTransactionReceipt(withdrawalTx.hash);
        const fee = receipt!.gasPrice * receipt!.gasUsed;

        const finalL1Balance = await alice.getBalanceL1(baseTokenDetails.l1Address);
        const finalL2Balance = await alice.getBalance();

        expect(finalL1Balance).toEqual(initialL1Balance + amount);
        expect(finalL2Balance + amount + fee).toEqual(initialL2Balance);
    }, 600000);

    test('Wrapped base token metadata', async () => {
        // This test is intended only to be run against newly created chains.
        if (!testMaster.isLocalHost()) {
            return;
        }

        let name;
        let symbol;

        if (isETHBasedChain) {
            name = 'Ether';
            symbol = 'ETH';
        } else {
            const contract = new ethers.Contract(baseTokenDetails.l1Address, zksync.utils.IERC20, alice.ethWallet());
            name = await contract.name();
            symbol = await contract.symbol();
        }

        const expectedWrappedBaseTokenName = `Wrapped ${name}`;
        const expectedWrappedBaseTokenSymbol = `W${symbol}`;

        const l2NativeTokenVault = IL2NativeTokenVault__factory.connect(
            zksync.utils.L2_NATIVE_TOKEN_VAULT_ADDRESS,
            alice
        );
        const wethToken = await l2NativeTokenVault.WETH_TOKEN();

        const wrappedBaseToken = new ethers.Contract(wethToken, zksync.utils.IERC20, alice);

        expect(expectedWrappedBaseTokenName).toEqual(await wrappedBaseToken.name());
        expect(expectedWrappedBaseTokenSymbol).toEqual(await wrappedBaseToken.symbol());
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
