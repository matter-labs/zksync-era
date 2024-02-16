/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src/index';
import { Token } from '../src/types';
import { shouldChangeTokenBalances, shouldOnlyTakeFee } from '../src/modifiers/balance-checker';

import * as zksync from 'zksync-web3';
import { BigNumber, utils as etherUtils } from 'ethers';
import * as ethers from 'ethers';
import { scaledGasPrice, waitUntilBlockFinalized } from '../src/helpers';
import { L2_ETH_PER_ACCOUNT } from '../src/context-owner';

describe('ERC20 contract checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let tokenDetails: Token;
    let nonNativeToken: Token;
    let aliceErc20: zksync.Contract;
    let isNativeErc20: boolean;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();

        tokenDetails = testMaster.environment().erc20Token;
        nonNativeToken = testMaster.environment().nonNativeToken;
        aliceErc20 = new zksync.Contract(tokenDetails.l2Address, zksync.utils.IERC20, alice);
        isNativeErc20 = testMaster.environment().nativeErc20Testing;
    });

    test('Can perform a deposit', async () => {
        const tokenAddress = nonNativeToken.l1Address;
        const amount = BigNumber.from(555);
        const gasPrice = scaledGasPrice(alice);
        const nativeToken = '0xD47a77a7A93c099a451a2c269ea5fB35238dC52c';

        // The non native token address should be different than the native token address.
        expect(tokenAddress != tokenDetails.l1Address);

        // L1 Deposit
        const initialTokenBalance = await alice.getBalanceL1(tokenAddress);

        const l2TokenAddress = await (
            await alice.getL1BridgeContracts()
        ).erc20['l2TokenAddress(address)'](tokenAddress);
        const initiall2BalanceThroughL1Bridge = await alice.getBalance(l2TokenAddress);

        const deposit = await alice.deposit(
            {
                token: tokenAddress,
                amount,
                to: alice.address,
                approveERC20: true,
                approveOverrides: {
                    gasPrice
                },
                overrides: {
                    gasPrice
                }
            },
            isNativeErc20 ? nativeToken : undefined
        );

        await deposit.waitFinalize();

        const finalTokenBalance = await alice.getBalanceL1(tokenAddress);

        /// Try through alice.l2TokenAddress
        const aliceL2TokenAddress = await alice.l2TokenAddress(tokenAddress);
        const aliceBalanceThroughAliceL2TokenAddress = await alice.getBalance(aliceL2TokenAddress);

        /// Try through l1ERC20Bridge.l2TokenAddress call
        const l2BalanceThroughL1Bridge = await alice.getBalance(l2TokenAddress);

        expect(initialTokenBalance.sub(finalTokenBalance)).toEqual(BigNumber.from(amount));
        expect(l2BalanceThroughL1Bridge.sub(initiall2BalanceThroughL1Bridge)).toEqual(amount);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
