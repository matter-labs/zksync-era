/**
 * This suite contains tests checking default ERC-20 contract behavior.
 */

import { TestMaster } from '../src/index';
import { Token } from '../src/types';

import * as zksync from 'zksync-ethers';
import { BigNumber } from 'ethers';
import * as ethers from 'ethers';
import { scaledGasPrice } from '../src/helpers';

describe('base ERC20 contract checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let baseTokenDetails: Token;
    let aliceBaseErc20: ethers.Contract;
    let chainId: ethers.BigNumberish;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        chainId = process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!;

        baseTokenDetails = testMaster.environment().baseToken;
        aliceBaseErc20 = new ethers.Contract(baseTokenDetails.l1Address, zksync.utils.IERC20, alice._providerL1());
    });

    test('Can perform a deposit', async () => {
        const amount = 1; // 1 wei is enough.
        const gasPrice = scaledGasPrice(alice);

        const initialEthBalance = await alice.getBalanceL1();
        const initialL1Balance = await alice.getBalanceL1(baseTokenDetails.l1Address);
        const initialL2Balance = await alice.getBalance();

        const depositTx = await alice.deposit({
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
        });
        const depositHash = depositTx.hash;
        await depositTx.wait();

        const receipt = await alice._providerL1().getTransactionReceipt(depositHash);
        const fee = receipt.effectiveGasPrice.mul(receipt.gasUsed);

        // TODO: should all the following tests use strict equality?

        const finalEthBalance = await alice.getBalanceL1();
        expect(initialEthBalance).bnToBeGt(finalEthBalance.add(fee)); // Fee should be taken from the ETH balance on L1.

        const finalL1Balance = await alice.getBalanceL1(baseTokenDetails.l1Address);
        expect(initialL1Balance).bnToBeGte(finalL1Balance.add(amount));

        const finalL2Balance = await alice.getBalance();
        expect(initialL2Balance).bnToBeLte(finalL2Balance.add(amount));
    });

    test('Not enough balance should revert', async () => {
        const amount = BigNumber.from('0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffff');
        const gasPrice = scaledGasPrice(alice);
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

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
