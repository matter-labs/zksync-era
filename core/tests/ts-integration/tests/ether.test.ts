/**
 * This suite contains tests checking our handling of Ether (such as depositing, checking `msg.value`, etc).
 */

import { TestMaster } from '../src/index';
import {
    shouldChangeETHBalances,
    shouldChangeTokenBalances,
    shouldOnlyTakeFee
} from '../src/modifiers/balance-checker';
import { checkReceipt } from '../src/modifiers/receipt-check';

import * as zksync from 'zksync-ethers';
import { BigNumber, Overrides } from 'ethers';
import { scaledGasPrice } from '../src/helpers';
import {
    EIP712_TX_TYPE,
    ETH_ADDRESS,
    ETH_ADDRESS_IN_CONTRACTS,
    REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT
} from 'zksync-ethers/build/utils';

describe('ETH token checks', () => {
    let testMaster: TestMaster;
    let alice: zksync.Wallet;
    let bob: zksync.Wallet;
    let isETHBasedChain: boolean;
    let l2EthTokenAddressNonBase: string; // Used only for base token implementation
    let baseTokenAddress: string; // Used only for base token implementation

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();
        bob = testMaster.newEmptyAccount();
        // Get the information about base token address directly from the L2.
        baseTokenAddress = await alice._providerL2().getBaseTokenContractAddress();
        isETHBasedChain = baseTokenAddress == ETH_ADDRESS_IN_CONTRACTS;
        console.log(`Starting checks for base token: ${baseTokenAddress} isEthBasedChain: ${isETHBasedChain}`);
        l2EthTokenAddressNonBase = await alice.l2TokenAddress(ETH_ADDRESS_IN_CONTRACTS);
    });

    test('Can perform a deposit', async () => {
        if (!isETHBasedChain) {
            // Approving the needed allowance previously so we don't do it inside of the deposit.
            // This prevents the deposit fee from being miscalculated.
            const l1MaxBaseTokenBalance = await alice.getBalanceL1(baseTokenAddress);
            await (await alice.approveERC20(baseTokenAddress, l1MaxBaseTokenBalance)).wait();
        }
        const amount = 1; // 1 wei is enough.
        const gasPrice = scaledGasPrice(alice);

        // Unfortunately, since fee is taken in ETH, we must calculate the L1 ETH balance diff explicitly.
        const l1EthBalanceBefore = await alice.getBalanceL1();
        const l2ethBalanceChange = isETHBasedChain
            ? await shouldChangeETHBalances([{ wallet: alice, change: amount }], {
                  l1ToL2: true
              })
            : await shouldChangeTokenBalances(l2EthTokenAddressNonBase, [{ wallet: alice, change: amount }]);

        // Variables used only for base token implementation
        const l1BaseTokenBalanceBefore = await alice.getBalanceL1(baseTokenAddress);
        const l2BaseTokenBalanceBefore = await alice.getBalance(); // Base token balance on L2

        const gasPerPubdataByte = REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_LIMIT;

        const l2GasLimit = await zksync.utils.estimateDefaultBridgeDepositL2Gas(
            alice.providerL1!,
            alice.provider,
            ETH_ADDRESS,
            amount,
            alice.address,
            alice.address,
            gasPerPubdataByte
        );
        const expectedL2Costs = await alice.getBaseCost({
            gasLimit: l2GasLimit,
            gasPerPubdataByte,
            gasPrice: await gasPrice
        });

        const depositOp = alice.deposit({
            token: ETH_ADDRESS,
            amount,
            gasPerPubdataByte,
            l2GasLimit,
            approveERC20: isETHBasedChain ? true : false,
            approveBaseOverrides: {
                gasPrice
            },
            overrides: {
                gasPrice
            }
        });
        await expect(depositOp).toBeAccepted([l2ethBalanceChange]);

        const depositFee = await depositOp
            .then((op) => op.waitL1Commit())
            .then(async (receipt) => {
                const l1GasFee = receipt.gasUsed.mul(receipt.effectiveGasPrice);
                if (!isETHBasedChain) {
                    return l1GasFee;
                }
                return l1GasFee.add(expectedL2Costs);
            });

        const l1EthBalanceAfter = await alice.getBalanceL1();
        if (isETHBasedChain) {
            expect(l1EthBalanceBefore.sub(depositFee).sub(l1EthBalanceAfter)).bnToBeEq(amount);
        } else {
            // Base token checks
            const l1BaseTokenBalanceAfter = await alice.getBalanceL1(baseTokenAddress);
            expect(l1BaseTokenBalanceBefore).bnToBeEq(l1BaseTokenBalanceAfter.add(expectedL2Costs));

            const l2BaseTokenBalanceAfter = await alice.getBalance();
            expect(l1EthBalanceBefore).bnToBeEq(l1EthBalanceAfter.add(depositFee).add(amount));

            // L2 balance for the base token increases do to some "overminting" of the base token
            // We verify that the amount reduced on L1 is greater than the amount increased on L2
            // so that we are not generating tokens out of thin air
            const l1BaseTokenBalanceDiff = l1BaseTokenBalanceBefore.sub(l1BaseTokenBalanceAfter);
            const l2BaseTokenBalanceDiff = l2BaseTokenBalanceAfter.sub(l2BaseTokenBalanceBefore);
            expect(l1BaseTokenBalanceDiff).bnToBeGt(l2BaseTokenBalanceDiff);
        }
    });

    test('Can perform a transfer (legacy pre EIP-155)', async () => {
        const LEGACY_TX_TYPE = 0;
        const value = BigNumber.from(200);

        const ethBalanceChange = await shouldChangeETHBalances([
            { wallet: alice, change: -value },
            { wallet: bob, change: value }
        ]);
        const correctReceiptType = checkReceipt(
            (receipt) => receipt.type == LEGACY_TX_TYPE,
            'Incorrect tx type in receipt'
        );

        // ethers doesn't support sending pre EIP-155 transactions, so we create one manually.
        const transaction = await alice.populateTransaction({ type: LEGACY_TX_TYPE, to: bob.address, value });
        // Remove chainId and sign the transaction without it.
        transaction.chainId = undefined;
        const signedTransaction = await alice.signTransaction(transaction);
        await expect(alice.provider.sendTransaction(signedTransaction)).toBeAccepted([
            ethBalanceChange,
            correctReceiptType
        ]);
    });

    test('Can perform a transfer (legacy EIP-155)', async () => {
        const LEGACY_TX_TYPE = 0;
        const value = BigNumber.from(200);

        const ethBalanceChange = await shouldChangeETHBalances([
            { wallet: alice, change: -value },
            { wallet: bob, change: value }
        ]);
        const correctReceiptType = checkReceipt(
            (receipt) => receipt.type == LEGACY_TX_TYPE,
            'Incorrect tx type in receipt'
        );

        await expect(alice.sendTransaction({ type: LEGACY_TX_TYPE, to: bob.address, value })).toBeAccepted([
            ethBalanceChange,
            correctReceiptType
        ]);
    });

    test('Can perform a transfer (EIP712)', async () => {
        const value = BigNumber.from(200);

        const ethBalanceChange = await shouldChangeETHBalances([
            { wallet: alice, change: -value },
            { wallet: bob, change: value }
        ]);
        const correctReceiptType = checkReceipt(
            (receipt) => receipt.type == EIP712_TX_TYPE,
            'Incorrect tx type in receipt'
        );

        await expect(alice.sendTransaction({ type: EIP712_TX_TYPE, to: bob.address, value })).toBeAccepted([
            ethBalanceChange,
            correctReceiptType
        ]);
    });

    test('Can perform a transfer (EIP1559)', async () => {
        const EIP1559_TX_TYPE = 2;
        const value = BigNumber.from(200);

        const ethBalanceChange = await shouldChangeETHBalances([
            { wallet: alice, change: -value },
            { wallet: bob, change: value }
        ]);
        const correctReceiptType = checkReceipt(
            (receipt) => receipt.type == EIP1559_TX_TYPE,
            'Incorrect tx type in receipt'
        );

        await expect(alice.sendTransaction({ type: EIP1559_TX_TYPE, to: bob.address, value })).toBeAccepted([
            ethBalanceChange,
            correctReceiptType
        ]);
    });

    test('Should reject transactions with access lists', async () => {
        const EIP_2930_TX_TYPE = 0x01;
        const EIP_1559_TX_TYPE = 0x02;
        const value = BigNumber.from(200);

        await expect(alice.sendTransaction({ type: EIP_2930_TX_TYPE, to: bob.address, value })).toBeRejected(
            'access lists are not supported'
        );

        await expect(
            alice.sendTransaction({
                type: EIP_1559_TX_TYPE,
                to: bob.address,
                value,
                accessList: [{ address: '0x0000000000000000000000000000000000000000', storageKeys: [] }]
            })
        ).toBeRejected('access lists are not supported');
    });

    test('Can perform a transfer to self', async () => {
        const value = BigNumber.from(200);

        // Balance should not change, only fee should be taken.
        const ethBalanceChange = await shouldOnlyTakeFee(alice);
        await expect(alice.sendTransaction({ to: alice.address, value })).toBeAccepted([ethBalanceChange]);
    });

    test('Incorrect transfer should revert', async () => {
        // Attempt to transfer the whole Alice balance: there would be no enough balance to cover the fee.
        const value = await alice.getBalance();

        // Since gas estimation is expected to fail, we request gas limit for similar non-failing tx.
        const gasLimit = await alice.estimateGas({ to: bob.address, value: 1 });

        // Send transfer, it should be rejected due to lack of balance.
        await expect(alice.sendTransaction({ to: bob.address, value, gasLimit })).toBeRejected(
            'insufficient funds for gas + value.'
        );
    });

    test('Can perform a withdrawal', async () => {
        if (!isETHBasedChain) {
            // TODO(EVM-555): Currently this test is not working for non-eth based chains.
            return;
        }
        if (testMaster.isFastMode()) {
            return;
        }
        const amount = 1;

        const l2ethBalanceChange = isETHBasedChain
            ? await shouldChangeETHBalances([{ wallet: alice, change: -amount }])
            : await shouldChangeTokenBalances(l2EthTokenAddressNonBase, [{ wallet: alice, change: -amount }]);

        const withdrawalPromise = alice.withdraw({
            token: isETHBasedChain ? ETH_ADDRESS : l2EthTokenAddressNonBase,
            amount
        });
        await expect(withdrawalPromise).toBeAccepted([l2ethBalanceChange]);
        const withdrawalTx = await withdrawalPromise;
        await withdrawalTx.waitFinalize();

        // TODO (SMA-1374): Enable L1 ETH checks as soon as they're supported.
        await expect(alice.finalizeWithdrawal(withdrawalTx.hash)).toBeAccepted();
        const tx = await alice.provider.getTransactionReceipt(withdrawalTx.hash);

        expect(tx.l2ToL1Logs[0].txIndexInL1Batch).toEqual(expect.anything());
    });

    test('Can perform a deposit with precalculated max value', async () => {
        if (!isETHBasedChain) {
            const baseTokenDetails = testMaster.environment().baseToken;
            const baseTokenMaxAmount = await alice.getBalanceL1(baseTokenDetails.l1Address);
            await (await alice.approveERC20(baseTokenAddress, baseTokenMaxAmount)).wait();
        }

        const depositFee = await alice.getFullRequiredDepositFee({
            token: ETH_ADDRESS
        });
        const l1Fee = depositFee.l1GasLimit.mul(depositFee.maxFeePerGas! || depositFee.gasPrice!);
        const l2Fee = depositFee.baseCost;
        const maxAmount = isETHBasedChain
            ? (await alice.getBalanceL1()).sub(l1Fee).sub(l2Fee)
            : (await alice.getBalanceL1()).sub(l1Fee); // l2Fee is paid in base token

        // Approving the needed allowance to ensure that the user has enough funds.
        const l2ethBalanceChange = isETHBasedChain
            ? await shouldChangeETHBalances([{ wallet: alice, change: maxAmount }], {
                  l1ToL2: true
              })
            : await shouldChangeTokenBalances(l2EthTokenAddressNonBase, [{ wallet: alice, change: maxAmount }]);

        const overrides: Overrides = depositFee.gasPrice
            ? { gasPrice: depositFee.gasPrice }
            : {
                  maxFeePerGas: depositFee.maxFeePerGas,
                  maxPriorityFeePerGas: depositFee.maxPriorityFeePerGas
              };
        overrides.gasLimit = depositFee.l1GasLimit;

        const depositOp = await alice.deposit({
            token: ETH_ADDRESS,
            amount: maxAmount,
            l2GasLimit: depositFee.l2GasLimit,
            approveBaseERC20: true,
            approveERC20: true,
            overrides
        });

        await expect(depositOp).toBeAccepted([l2ethBalanceChange]);
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
