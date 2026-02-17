/**
 * This suite contains tests checking Interop-B bundles features,
 * those that were included as part of v31 protocol upgrade.
 *
 * TODO: do mention here the balance checks being done and at which steps:
 *  before sending, after sending, after finalized sending, after receiving,
 */

import { AssetTrackerBalanceSnapshot, InteropTestContext } from '../src/interop-setup';
import { formatEvmV1Address } from '../src/helpers';
import { L2_ASSET_ROUTER_ADDRESS, L2_NATIVE_TOKEN_VAULT_ADDRESS } from '../src/constants';
import { TransactionReceipt, MaxUint256 } from 'ethers';
import type { Overrides } from 'ethers';

describe('Interop-B Bundles behavior checks', () => {
    const ctx = new InteropTestContext();

    // Stored bundle data for cross-test assertions
    const bundles: Record<string, { amounts: string[]; receipt?: TransactionReceipt }> = {};
    const executedInterop2Blocks: number[] = [];
    let trackerSnapshotBefore: AssetTrackerBalanceSnapshot;
    let trackerSnapshotAfterSend: AssetTrackerBalanceSnapshot;
    let totalBaseAmount = 0n;
    let totalTokenAmount = 0n;
    let totalMsgValue = 0n;

    beforeAll(async () => {
        await ctx.initialize(__filename);
        if (ctx.skipInteropTests) return;

        await ctx.performSharedSetup();
    });

    // We send all test bundles at once to make testing faster.
    // This way, we don't have to wait for each bundle to reach Chain B separately.
    test('Can send bundles', async () => {
        if (ctx.skipInteropTests) return;

        const interopRequests: Record<string, Promise<TransactionReceipt>> = {};
        trackerSnapshotBefore = await ctx.snapshotInteropAssetTrackerBalances();

        await (await ctx.interop1TokenA.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, MaxUint256)).wait();
        const balanceBefore = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
        const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

        let nextNonce = await ctx.interop1Wallet.getNonce();
        const withNonce = (overrides: Overrides = {}): Overrides => ({
            ...overrides,
            nonce: nextNonce++
        });

        // SINGLE DIRECT CALL BUNDLE
        // Simple transfer
        {
            const amount = ctx.getTransferAmount();

            const msgValue = ctx.isSameBaseToken ? amount : 0n;
            totalMsgValue += msgValue;
            interopRequests.singleDirect = ctx.fromInterop1RequestInterop(
                [
                    {
                        to: formatEvmV1Address(ctx.dummyInteropRecipient),
                        data: '0x',
                        callAttributes: [ctx.interopCallValueAttr(amount)]
                    }
                ],
                { executionAddress: ctx.interop2RichWallet.address },
                withNonce({ value: msgValue })
            );

            bundles.singleDirect = { amounts: [amount.toString()] };
            totalBaseAmount += amount;
        }

        // SINGLE INDIRECT CALL BUNDLE
        // Simple token transfer
        {
            const amount = await ctx.getTransferAmount();

            interopRequests.singleIndirect = ctx.fromInterop1RequestInterop(
                [
                    {
                        to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                        data: ctx.getTokenTransferSecondBridgeData(
                            ctx.tokenA.assetId!,
                            amount,
                            ctx.interop2Recipient.address
                        ),
                        callAttributes: [ctx.indirectCallAttr()]
                    }
                ],
                undefined,
                withNonce()
            );

            bundles.singleIndirect = { amounts: [amount.toString()] };
            totalTokenAmount += amount;
        }

        // TWO DIRECT CALLS BUNDLE
        // Transfer to two different recipients
        {
            const baseAmountA = ctx.getTransferAmount();
            const baseAmountB = ctx.getTransferAmount();
            const totalAmount = baseAmountA + baseAmountB;

            const msgValue = ctx.isSameBaseToken ? totalAmount : 0n;
            totalMsgValue += msgValue;
            interopRequests.twoDirect = ctx.fromInterop1RequestInterop(
                [
                    {
                        to: formatEvmV1Address(ctx.dummyInteropRecipient),
                        data: '0x',
                        callAttributes: [ctx.interopCallValueAttr(baseAmountA)]
                    },
                    {
                        to: formatEvmV1Address(ctx.otherDummyInteropRecipient),
                        data: '0x',
                        callAttributes: [ctx.interopCallValueAttr(baseAmountB)]
                    }
                ],
                { executionAddress: ctx.interop2RichWallet.address },
                withNonce({ value: msgValue })
            );

            bundles.twoDirect = { amounts: [baseAmountA.toString(), baseAmountB.toString()] };
            totalBaseAmount += totalAmount;
        }

        // TWO INDIRECT CALLS BUNDLE
        // Two token transfers to different recipients
        {
            const tokenAmountA = await ctx.getTransferAmount();
            const tokenAmountB = await ctx.getTransferAmount();
            const totalAmount = tokenAmountA + tokenAmountB;

            interopRequests.twoIndirect = ctx.fromInterop1RequestInterop(
                [
                    {
                        to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                        data: ctx.getTokenTransferSecondBridgeData(
                            ctx.tokenA.assetId!,
                            tokenAmountA,
                            ctx.interop2Recipient.address
                        ),
                        callAttributes: [ctx.indirectCallAttr()]
                    },
                    {
                        to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                        data: ctx.getTokenTransferSecondBridgeData(
                            ctx.tokenA.assetId!,
                            tokenAmountB,
                            ctx.otherInterop2Recipient.address
                        ),
                        callAttributes: [ctx.indirectCallAttr()]
                    }
                ],
                undefined,
                withNonce()
            );

            bundles.twoIndirect = { amounts: [tokenAmountA.toString(), tokenAmountB.toString()] };
            totalTokenAmount += totalAmount;
        }

        // MIXED BUNDLE
        // One transfer and one token transfer
        {
            const baseAmount = ctx.getTransferAmount();
            const tokenAmount = await ctx.getTransferAmount();

            const msgValue = ctx.isSameBaseToken ? baseAmount : 0n;
            totalMsgValue += msgValue;
            interopRequests.mixed = ctx.fromInterop1RequestInterop(
                [
                    {
                        to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                        data: ctx.getTokenTransferSecondBridgeData(
                            ctx.tokenA.assetId!,
                            tokenAmount,
                            ctx.interop2Recipient.address
                        ),
                        callAttributes: [ctx.indirectCallAttr()]
                    },
                    {
                        to: formatEvmV1Address(ctx.dummyInteropRecipient),
                        data: '0x',
                        callAttributes: [ctx.interopCallValueAttr(baseAmount)]
                    }
                ],
                { executionAddress: ctx.interop2RichWallet.address },
                withNonce({ value: msgValue })
            );

            bundles.mixed = { amounts: [baseAmount.toString(), tokenAmount.toString()] };
            totalBaseAmount += baseAmount;
            totalTokenAmount += tokenAmount;
        }

        let totalFeePaid = 0n;
        // Send all bundles at once to make testing faster
        for (const bundleName of Object.keys(bundles)) {
            const receipt = await interopRequests[bundleName];
            totalFeePaid += BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
            bundles[bundleName].receipt = receipt;
        }

        // Asset tracker state does not change until the batches containing the bundles are executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotBefore, {});

        // Assert the balances after sending the bundles are as expected
        const balanceAfter = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
        if (ctx.isSameBaseToken) {
            expect(balanceAfter.toString()).toBe((balanceBefore - totalMsgValue - totalFeePaid).toString());
        } else {
            expect(balanceAfter.toString()).toBe((balanceBefore - totalBaseAmount).toString());
        }
        const tokenBalanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect((tokenBalanceAfter - tokenBalanceBefore).toString()).toBe((-totalTokenAmount).toString());

        // We wait for the last of these bundles to be executable on the destination chain.
        // By then, all of the bundles should be executable.
        await ctx.awaitInteropBundle(bundles.mixed.receipt!.hash);
        // Assert the asset tracker state changes after interop bundles are executed on GW, debiting the sender chain
        await ctx.assertAssetTrackerBalances(trackerSnapshotBefore, {
            gwatSenderBase: -totalBaseAmount,
            gwatSenderToken: -totalTokenAmount
        });
        // Update the snapshot after the bundles are executed on GW.
        trackerSnapshotAfterSend = await ctx.snapshotInteropAssetTrackerBalances();
    });

    test('Can receive a single direct call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(bundles.singleDirect.receipt!.hash);
        executedInterop2Blocks.push(execReceipt!.blockNumber);

        // Check the dummy interop recipient balance increased by the interop call value
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(bundles.singleDirect.amounts[0]);

        // Asset tracker state does not change until the batch containing the execution of the bundle is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    test('Can receive a single indirect call bundle', async () => {
        if (ctx.skipInteropTests) return;

        // Broadcast interop transaction from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(bundles.singleIndirect.receipt!.hash);
        executedInterop2Blocks.push(execReceipt!.blockNumber);
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

        // The balance before is 0 as the token did not yet exist on the second chain.
        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect(recipientBalance.toString()).toBe(bundles.singleIndirect.amounts[0]);

        // Asset tracker state does not change until the batch containing the execution of the bundle is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    test('Can receive a two direct call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        const otherRecipientBalanceBefore = await ctx.getInterop2Balance(ctx.otherDummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(bundles.twoDirect.receipt!.hash);
        executedInterop2Blocks.push(execReceipt!.blockNumber);

        // Check both recipients received their amounts
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(bundles.twoDirect.amounts[0]);
        const otherRecipientBalance = await ctx.getInterop2Balance(ctx.otherDummyInteropRecipient);
        expect((otherRecipientBalance - otherRecipientBalanceBefore).toString()).toBe(bundles.twoDirect.amounts[1]);

        // Asset tracker state does not change until the batch containing the execution of the bundle is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    test('Can receive a two indirect call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        const otherRecipientBalanceBefore = await ctx.getTokenBalance(
            ctx.otherInterop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );

        // Broadcast interop transaction from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(bundles.twoIndirect.receipt!.hash);
        executedInterop2Blocks.push(execReceipt!.blockNumber);

        // Check both recipients received their token amounts
        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(bundles.twoIndirect.amounts[0]);
        const otherRecipientBalance = await ctx.getTokenBalance(
            ctx.otherInterop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((otherRecipientBalance - otherRecipientBalanceBefore).toString()).toBe(bundles.twoIndirect.amounts[1]);

        // Asset tracker state does not change until the batch containing the execution of the bundle is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    test('Can receive a mixed call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        const recipientTokenBalanceBefore = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );

        // Broadcast interop transaction from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(bundles.mixed.receipt!.hash);
        executedInterop2Blocks.push(execReceipt!.blockNumber);

        // Check the dummy interop recipient balance increased by the interop call value
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(bundles.mixed.amounts[0]);
        // Check the token balance on the second chain increased by the token transfer amount
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((recipientTokenBalance - recipientTokenBalanceBefore).toString()).toBe(bundles.mixed.amounts[1]);

        // Asset tracker state does not change until the batch containing the execution of the bundle is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    test('Gateway balances after batch execution reflect executed bundles', async () => {
        if (ctx.skipInteropTests) return;

        const maxExecutedBlock = Math.max(...executedInterop2Blocks);
        await ctx.waitUntilInterop2BlockExecutedOnGateway(maxExecutedBlock);

        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend, {
            gwatReceiverBase: totalBaseAmount,
            gwatReceiverToken: totalTokenAmount
        });
    });

    afterAll(async () => {
        await ctx.deinitialize();
    });
});
