/**
 * This suite contains tests checking deposits.
 * Should have 2 main tests:
 1. One that does a regular valid deposit and checks that:
    - The native balance on the L2 increase by the amount deposited.
    - The ERC20 balance on the L1 decreased by that same amount plus a bit more (accounting for the operator fee).
    - The eth balance on the L1 decreased, but only to cover the deposit transaction fee on the L1.
 2. One that ensures that no one can deposit more money than they have.
 */

import { TestMaster } from '../src/index';
import { Token } from '../src/types';
import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

import * as zksync from 'zksync-web3';
import { BigNumber, utils as etherUtils } from 'ethers';
import * as ethers from 'ethers';
import { scaledGasPrice, waitUntilBlockFinalized } from '../src/helpers';
import { L2_ETH_PER_ACCOUNT } from '../src/context-owner';

async function get_wallet_balances(wallet: zksync.Wallet, tokenDetails: Token) {
    return {
        nativeTokenL2: await wallet.getBalance(),
        ethL1: await wallet.getBalanceL1(),
        nativeTokenL1: await wallet.getBalanceL1(tokenDetails.l1Address)
    };
}

describe('Deposit', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let tokenDetails: Token;
    let erc20: zksync.Contract;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename); // Configures env vars for the test.
        alice = testMaster.mainAccount(); // funded amount.
        bob = testMaster.newEmptyAccount(); // empty account.

        tokenDetails = testMaster.environment().erc20Token; // Contains the native token details.
        erc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, alice); //
    });

    test('Can perform a deposit', async () => {
        // Amount sending to the L2.
        const amount = 2836168500000000;
        const gasPrice = scaledGasPrice(alice);

        // Initil balance checking.
        const initialBalances = await get_wallet_balances(alice, tokenDetails);

        const deposit = await alice.deposit(
            {
                token: tokenDetails.l1Address,
                amount,
                approveERC20: true,
                approveOverrides: {
                    gasPrice
                },
                overrides: {
                    gasPrice
                }
            },
            tokenDetails.l1Address
        );
        await deposit.waitFinalize();

        // Final balance checking.
        const finalBalances = await get_wallet_balances(alice, tokenDetails);

        // Check that the balances are correct.
        expect(finalBalances.nativeTokenL2).bnToBeGt(initialBalances.nativeTokenL2.add(amount));
        expect(finalBalances.ethL1).bnToBeLt(initialBalances.ethL1);
        expect(finalBalances.nativeTokenL1).bnToBeLt(initialBalances.nativeTokenL1.sub(amount));
    });

    test('Not enough balance should revert', async () => {
        // Amount sending to the L2.
        const amount = BigNumber.from('0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff');
        const gasPrice = scaledGasPrice(alice);
        const gasLimit = 1_000_000_000_000;
        await expect(
            alice.deposit(
                {
                    token: tokenDetails.l1Address,
                    amount,
                    approveERC20: true,
                    approveOverrides: {
                        gasPrice,
                        gasLimit
                    },
                    overrides: {
                        gasPrice,
                        gasLimit
                    },
                    l2GasLimit: gasLimit
                },
                tokenDetails.l1Address
            )
        ).toBeRejected('Not enough balance');
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
