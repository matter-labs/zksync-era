/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src/index';
import { Token } from '../src/types';

import * as zksync from 'zksync-web3';
import * as ethers from 'ethers';
import { ETH_ADDRESS } from 'zksync-web3/build/src/utils';

describe('ERC20 contract checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let tokenDetails: Token;
    let aliceErc20: ethers.Contract;
    const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();

        tokenDetails = testMaster.environment().erc20Token;
        aliceErc20 = (await alice.getL1BridgeContracts()).erc20;
    });

    test('Can perform a valid withdrawal', async () => {
        // TODO: make sure that the test is not run in the fast mode
        if (testMaster.isFastMode()) {
            return;
        }
        const amount = 1;

        const initialBalanceL2 = await alice.getBalance();
        const initialBalanceL1 = await alice.getBalanceL1(tokenDetails.l1Address);

        // First, a withdraw transaction is done on the L2,
        const withdraw = await alice.withdraw({ token: ETH_ADDRESS, amount });
        const withdrawalHash = withdraw.hash;
        await withdraw.waitFinalize();

        // Get receipt of withdraw transaction, and check the gas cost (fee)
        const receipt = await alice.provider.getTransactionReceipt(withdrawalHash);
        const fee = receipt.effectiveGasPrice.mul(receipt.gasUsed);

        const finalBalanceL2 = await alice.getBalance();
        let expected = initialBalanceL2.sub(amount).sub(fee);
        let actual = finalBalanceL2;
        expect(actual == expected);

        // Afterwards, a withdraw-finalize is done on the L1,
        (await alice.finalizeWithdrawal(withdrawalHash)).wait();

        // make sure that the balance on the L1 has increased by the amount withdrawn
        const finalBalanceL1 = await alice.getBalanceL1(tokenDetails.l1Address);
        expected = initialBalanceL1.add(amount);
        actual = finalBalanceL1;
        expect(actual == expected);
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
            expect(err.includes('insufficient balance for transfer'));
        }
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
