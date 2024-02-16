/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src/index';
import { Token } from '../src/types';

import * as zksync from 'zksync-web3';
import * as ethers from 'ethers';
import { ETH_ADDRESS, L2_ETH_TOKEN_ADDRESS } from 'zksync-web3/build/src/utils';
import { shouldChangeTokenBalances } from '../src/modifiers/balance-checker';

describe('ERC20 contract checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let tokenDetails: Token;
    let aliceErc20: ethers.Contract;
    let isNativeErc20: boolean;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        isNativeErc20 = testMaster.environment().nativeErc20Testing;

        tokenDetails = testMaster.environment().erc20Token;
        aliceErc20 = (await alice.getL1BridgeContracts()).erc20;
    });

    test('Can perform a valid withdrawal', async () => {
        // TODO: make sure that the test is not run in the fast mode
        if (testMaster.isFastMode()) {
            return;
        }
        const amount = 500;
        let tokenAddress = isNativeErc20 ? tokenDetails.l1Address : undefined

        const l1ERC20InitialBalance = await alice.getBalanceL1(tokenAddress);
        const initialBalanceL2 = await alice.getBalance();
        // First, a withdraw transaction is done on the L2.
        const withdraw = await alice.withdraw({ token: L2_ETH_TOKEN_ADDRESS, amount });
        const withdrawalHash = withdraw.hash;
        await withdraw.waitFinalize();

        // Get receipt of withdraw transaction, and check the gas cost (fee)
        const receipt = await alice.provider.getTransactionReceipt(withdrawalHash);
        const fee = receipt.effectiveGasPrice.mul(receipt.gasUsed);

        const finalBalanceL2 = await alice.getBalance();
        let expected = initialBalanceL2.sub(amount).sub(fee);
        let actual = finalBalanceL2;
        expect(expected).toStrictEqual(actual);

        let tx = await alice.finalizeWithdrawal(withdrawalHash);
        await tx.wait();

        if (isNativeErc20) {
            expect(await alice.getBalanceL1(tokenAddress)).toEqual(l1ERC20InitialBalance.add(amount));
        } else {
            let l1Receipt = await alice._providerL1().getTransactionReceipt(tx.hash);
            const l1Fee = l1Receipt.effectiveGasPrice.mul(l1Receipt.gasUsed);
            expect(await alice.getBalanceL1(tokenAddress)).toEqual(l1ERC20InitialBalance.add(amount).sub(l1Fee));
        }
    });

    test(`Can't perform an invalid withdrawal`, async () => {
        // TODO: make sure that the test is not run in the fast mode
        if (testMaster.isFastMode()) {
            return;
        }

        const initialBalanceL2 = await alice.getBalance();
        const amount = initialBalanceL2.add(1);
        try {
            await alice.withdraw({ token: ETH_ADDRESS, amount });
        } catch (e: any) {
            const err = e.toString();
            expect(err.includes('insufficient balance for transfer')).toBe(true);
        }
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
