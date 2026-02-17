/**
 * This suite contains tests checking Interop-B unbundle features,
 * those that were included as part of v32 protocol upgrade.
 *
 * This suite checks Asset Tracker accounting for Interop-B unbundle (v32):
 * - Baseline GWAT/L2AssetTracker balances before sending.
 * - After sending on source (before GW batch): L2AssetTracker and GWAT unchanged.
 * - After gateway executes source batch: GWAT sender decreases (L2AssetTracker unchanged).
 * - After destination executes unbundle (before GW batch): GWAT unchanged.
 * - After gateway executes destination batch: GWAT receiver increases only for executed calls.
 *
 * Additional checks specific to unbundling:
 * - Progressive unbundling updates GWAT per each unbundling call marked as "Executed".
 * - A cancelled call is expected to leave funds unrecoverable until a recovery mechanism is implemented.
 *   These lost funds are NOT reflected on the GW Asset Tracker.
 */

import * as zksync from 'zksync-ethers';
import { AssetTrackerBalanceSnapshot, InteropTestContext } from '../src/interop-setup';
import { formatEvmV1Address, waitForL2ToL1LogProof } from '../src/helpers';
import {
    ArtifactInteropHandler,
    L2_ASSET_ROUTER_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS
} from '../src/constants';
import { MaxUint256, Overrides, TransactionReceipt } from 'ethers';
import { BundleStatus, CallStatus, getInteropBundleData, Output } from '../src/temp-sdk';

describe('Interop-B Unbundle behavior checks', () => {
    const ctx = new InteropTestContext();

    // Stored bundle data for cross-test assertions
    const bundles: Record<
        string,
        {
            receipt?: TransactionReceipt;
            amounts?: { baseAmount: string; tokenAmount: string };
            data?: Output;
        }
    > = {};
    const executedInterop2Blocks: number[] = [];
    let trackerSnapshotBefore: AssetTrackerBalanceSnapshot;
    let trackerSnapshotAfterSend: AssetTrackerBalanceSnapshot;
    let totalBaseAmount = 0n;
    let totalTokenAmount = 0n;
    let totalMsgValue = 0n;

    // Failing call is some random call to a contract that doesn't exist
    const failingCallContract = '0x000000000000000000000000000000000000feed';
    const failingCallCalldata = '0x00056d83'; // selector: "someVeryUnfortunateCall()"

    // Final call status after unbundling
    const finalCallStatusesFromDestination = [CallStatus.Executed, CallStatus.Cancelled, CallStatus.Executed];
    const finalCallStatusesFromSource = [CallStatus.Executed, CallStatus.Unprocessed, CallStatus.Cancelled];

    beforeAll(async () => {
        await ctx.initialize(__filename);
        if (ctx.skipInteropTests) return;

        await ctx.performSharedSetup();
    });

    // We send all test bundles at once to make testing faster.
    // This way, we don't have to wait for each bundle to reach Chain B separately.
    test('Can send bundles that need unbundling', async () => {
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

        // FAILING BUNDLE THAT WILL BE UNBUNDLED FROM THE DESTINATION CHAIN
        // One transfer, one failing call, and one token transfer
        // Additionally sets a specific unbundler address
        {
            const baseAmount = ctx.getTransferAmount();
            const tokenAmount = await ctx.getTransferAmount();
            totalBaseAmount += baseAmount;
            totalTokenAmount += tokenAmount;

            const msgValue = ctx.isSameBaseToken ? baseAmount : 0n;
            totalMsgValue += msgValue;
            interopRequests.fromDestinationChain = await ctx.fromInterop1RequestInterop(
                [
                    {
                        to: formatEvmV1Address(ctx.dummyInteropRecipient),
                        data: '0x',
                        callAttributes: [ctx.interopCallValueAttr(baseAmount)]
                    },
                    {
                        to: formatEvmV1Address(failingCallContract),
                        data: failingCallCalldata,
                        callAttributes: []
                    },
                    {
                        to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                        data: ctx.getTokenTransferSecondBridgeData(
                            ctx.tokenA.assetId!,
                            tokenAmount,
                            ctx.interop2Recipient.address
                        ),
                        callAttributes: [ctx.indirectCallAttr()]
                    }
                ],
                { executionAddress: ctx.interop2RichWallet.address, unbundlerAddress: ctx.interop2RichWallet.address },
                withNonce({ value: msgValue })
            );

            bundles.fromDestinationChain = {
                amounts: { baseAmount: baseAmount.toString(), tokenAmount: tokenAmount.toString() }
            };
        }

        // FAILING BUNDLE THAT WILL BE UNBUNDLED FROM THE SOURCE CHAIN
        // One transfer, one failing call, and one token transfer
        {
            const baseAmount = ctx.getTransferAmount();
            const tokenAmount = await ctx.getTransferAmount();
            totalBaseAmount += baseAmount;
            totalTokenAmount += tokenAmount;

            const msgValue = ctx.isSameBaseToken ? baseAmount : 0n;
            totalMsgValue += msgValue;
            interopRequests.fromSourceChain = await ctx.fromInterop1RequestInterop(
                [
                    {
                        to: formatEvmV1Address(ctx.dummyInteropRecipient),
                        data: '0x',
                        callAttributes: [ctx.interopCallValueAttr(baseAmount)]
                    },
                    {
                        to: formatEvmV1Address(failingCallContract),
                        data: failingCallCalldata,
                        callAttributes: []
                    },
                    {
                        to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                        data: ctx.getTokenTransferSecondBridgeData(
                            ctx.tokenA.assetId!,
                            tokenAmount,
                            ctx.interop2Recipient.address
                        ),
                        callAttributes: [ctx.indirectCallAttr()]
                    }
                ],
                // The unbundler address defaults to the sending address on the destination chain
                { executionAddress: ctx.interop2RichWallet.address },
                withNonce({ value: msgValue })
            );

            bundles.fromSourceChain = {
                amounts: { baseAmount: baseAmount.toString(), tokenAmount: tokenAmount.toString() }
            };
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

        // We need to wait for bundle data to be available
        await waitForL2ToL1LogProof(
            ctx.interop1Wallet,
            bundles.fromSourceChain.receipt!.blockNumber,
            bundles.fromSourceChain.receipt!.hash
        );

        // Store the bundle data
        bundles.fromDestinationChain.data = await getInteropBundleData(
            ctx.interop1Provider,
            bundles.fromDestinationChain.receipt!.hash,
            0
        );
        bundles.fromSourceChain.data = await getInteropBundleData(
            ctx.interop1Provider,
            bundles.fromSourceChain.receipt!.hash,
            0
        );

        // UNBUNDLING BUNDLE FROM THE SOURCE CHAIN
        // We send another bundle from the source chain, verifying the failing bundle and then unbundling it
        // We send this bundle directly to make testing faster.
        {
            const verifyBundleData = ctx.interop2InteropHandler.interface.encodeFunctionData('verifyBundle', [
                bundles.fromSourceChain.data!.rawData,
                bundles.fromSourceChain.data!.proofDecoded
            ]);
            const unbundleBundleData = ctx.interop2InteropHandler.interface.encodeFunctionData('unbundleBundle', [
                ctx.interop1ChainId,
                bundles.fromSourceChain.data!.rawData,
                finalCallStatusesFromSource
            ]);
            const receipt = await ctx.fromInterop1RequestInterop(
                [
                    {
                        to: formatEvmV1Address(L2_INTEROP_HANDLER_ADDRESS),
                        data: verifyBundleData,
                        callAttributes: []
                    },
                    {
                        to: formatEvmV1Address(L2_INTEROP_HANDLER_ADDRESS),
                        data: unbundleBundleData,
                        callAttributes: []
                    }
                ],
                { executionAddress: ctx.interop2RichWallet.address, unbundlerAddress: ctx.interop2RichWallet.address }
            );

            bundles.unbundlingBundleReceipt = { receipt };
        }

        // We wait for the last of these bundles to be executable on the destination chain.
        // By then, all of the bundles should be executable.
        await ctx.awaitInteropBundle(bundles.unbundlingBundleReceipt.receipt!.hash);
        // Assert the asset tracker state changes after interop bundles are executed on GW, debiting the sender chain
        await ctx.assertAssetTrackerBalances(trackerSnapshotBefore, {
            gwatSenderBase: -totalBaseAmount,
            gwatSenderToken: -totalTokenAmount
        });
        // Update the snapshot after the bundles are executed on GW.
        trackerSnapshotAfterSend = await ctx.snapshotInteropAssetTrackerBalances();
    });

    test('Cannot unbundle a non-verified bundle', async () => {
        if (ctx.skipInteropTests) return;

        await expect(
            ctx.interop2InteropHandler.unbundleBundle(ctx.interop1ChainId, bundles.fromDestinationChain.data!.rawData, [
                CallStatus.Executed,
                CallStatus.Cancelled,
                CallStatus.Executed
            ])
        ).rejects.toThrow();
    });

    test('Can verify a bundle', async () => {
        if (ctx.skipInteropTests) return;

        // Try to execute atomically - should revert
        await expect(ctx.readAndBroadcastInteropBundle(bundles.fromDestinationChain.receipt!.hash)).rejects.toThrow();

        // Bundle is still Unreceived, so verify it so it's ready for unbundling
        const receipt = await ctx.interop2InteropHandler.verifyBundle(
            bundles.fromDestinationChain.data!.rawData,
            bundles.fromDestinationChain.data!.proofDecoded
        );
        await receipt.wait();

        // Bundle is now marked as verified
        expect(
            (await ctx.interop2InteropHandler.bundleStatus(bundles.fromDestinationChain.data!.bundleHash)).toString()
        ).toBe(BundleStatus.Verified.toString());
    });

    test('Cannot unbundle from the wrong unbundler address', async () => {
        if (ctx.skipInteropTests) return;

        const altInterop2InteropHandler = new zksync.Contract(
            L2_INTEROP_HANDLER_ADDRESS,
            ArtifactInteropHandler.abi,
            ctx.interop2Recipient
        );
        await expect(
            altInterop2InteropHandler.unbundleBundle(ctx.interop1ChainId, bundles.fromDestinationChain.data!.rawData, [
                CallStatus.Executed,
                CallStatus.Unprocessed,
                CallStatus.Executed
            ])
        ).rejects.toThrow();
    });

    test('Cannot unbundle a failing call', async () => {
        if (ctx.skipInteropTests) return;
        await expect(
            ctx.interop2InteropHandler.unbundleBundle(ctx.interop1ChainId, bundles.fromDestinationChain.data!.rawData, [
                CallStatus.Unprocessed,
                CallStatus.Executed,
                CallStatus.Unprocessed
            ])
        ).rejects.toThrow();
    });

    test('Can unbundle from the destination chain', async () => {
        if (ctx.skipInteropTests) return;
        const bundleHash = bundles.fromDestinationChain.data!.bundleHash;

        // PROGRESSIVE UNBUNDLING
        // Leave call 0 as unprocessed (base token transfer), cancel call 1, and execute call 2 (token transfer)
        const firstCallStatuses = [CallStatus.Unprocessed, CallStatus.Cancelled, CallStatus.Executed];
        const firstUnbundleReceipt = await (
            await ctx.interop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                bundles.fromDestinationChain.data!.rawData,
                firstCallStatuses
            )
        ).wait();
        executedInterop2Blocks.push(firstUnbundleReceipt.blockNumber);
        // Balance checks
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);
        const tokenBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect(tokenBalance.toString()).toBe(bundles.fromDestinationChain.amounts!.tokenAmount);
        // Asset tracker state does not change until the batch containing the first unbundle is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
        // Bundle is now Unbundled
        expect((await ctx.interop2InteropHandler.bundleStatus(bundleHash)).toString()).toBe(
            BundleStatus.Unbundled.toString()
        );
        // Calls were processed as specified
        for (const [index, callStatus] of firstCallStatuses.entries()) {
            expect((await ctx.interop2InteropHandler.callStatus(bundleHash, index)).toString()).toBe(
                callStatus.toString()
            );
        }
        // GWAT reflects this partial unbundle
        await ctx.waitUntilInterop2BlockExecutedOnGateway(firstUnbundleReceipt.blockNumber);
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend, {
            gwatReceiverToken: BigInt(bundles.fromDestinationChain.amounts!.tokenAmount)
        });
        // Take new snapshot
        trackerSnapshotAfterSend = await ctx.snapshotInteropAssetTrackerBalances();

        // PROGRESSIVE UNBUNDLING
        // Unbundle again and process call 0 (base token transfer)
        const balanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        const secondUnbundleReceipt = await (
            await ctx.interop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                bundles.fromDestinationChain.data!.rawData,
                [CallStatus.Executed, CallStatus.Unprocessed, CallStatus.Unprocessed]
            )
        ).wait();
        executedInterop2Blocks.push(secondUnbundleReceipt.blockNumber);
        // Balance checks
        const balance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((balance - balanceBefore).toString()).toBe(bundles.fromDestinationChain.amounts!.baseAmount);
        // Asset tracker state does not change until the batch containing the second unbundle is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
        // Bundle remains Unbundled
        expect((await ctx.interop2InteropHandler.bundleStatus(bundleHash)).toString()).toBe(
            BundleStatus.Unbundled.toString()
        );
        // Calls were processed as specified
        for (const [index, callStatus] of finalCallStatusesFromDestination.entries()) {
            expect((await ctx.interop2InteropHandler.callStatus(bundleHash, index)).toString()).toBe(
                callStatus.toString()
            );
        }
    });

    test('Cannot unbundle a processed call', async () => {
        if (ctx.skipInteropTests) return;
        await expect(
            ctx.interop2InteropHandler.unbundleBundle(ctx.interop1ChainId, bundles.fromDestinationChain.data!.rawData, [
                CallStatus.Executed,
                CallStatus.Cancelled,
                CallStatus.Executed
            ])
        ).rejects.toThrow();
    });

    test('Cannot unbundle a cancelled call', async () => {
        if (ctx.skipInteropTests) return;
        await expect(
            ctx.interop2InteropHandler.unbundleBundle(ctx.interop1ChainId, bundles.fromDestinationChain.data!.rawData, [
                CallStatus.Unprocessed,
                CallStatus.Executed,
                CallStatus.Unprocessed
            ])
        ).rejects.toThrow();
    });

    test('Cannot cancel a processed call', async () => {
        if (ctx.skipInteropTests) return;
        await expect(
            ctx.interop2InteropHandler.unbundleBundle(ctx.interop1ChainId, bundles.fromDestinationChain.data!.rawData, [
                CallStatus.Cancelled,
                CallStatus.Cancelled,
                CallStatus.Cancelled
            ])
        ).rejects.toThrow();
    });

    test('Can send an unbundling bundle from the source chain', async () => {
        if (ctx.skipInteropTests) return;
        const bundleHash = bundles.fromSourceChain.data!.bundleHash;

        const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        const balanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast unbundling bundle from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(bundles.unbundlingBundleReceipt.receipt!.hash);
        executedInterop2Blocks.push(execReceipt!.blockNumber);

        // Balance checks
        const tokenBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        // Third call was set as cancelled, so no funds will be credited
        expect((tokenBalance - tokenBalanceBefore).toString()).toBe('0');
        const balance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((balance - balanceBefore).toString()).toBe(bundles.fromSourceChain.amounts!.baseAmount);
        // Asset tracker state does not change until the batch containing the unbundling bundle is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
        // Bundle is now Unbundled
        expect((await ctx.interop2InteropHandler.bundleStatus(bundleHash)).toString()).toBe(
            BundleStatus.Unbundled.toString()
        );
        // Calls were processed as specified
        for (const [index, callStatus] of finalCallStatusesFromSource.entries()) {
            expect((await ctx.interop2InteropHandler.callStatus(bundleHash, index)).toString()).toBe(
                callStatus.toString()
            );
        }
    });

    test('Gateway balances after batch execution reflect executed unbundles', async () => {
        if (ctx.skipInteropTests) return;

        const maxExecutedBlock = Math.max(...executedInterop2Blocks);
        await ctx.waitUntilInterop2BlockExecutedOnGateway(maxExecutedBlock);

        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend, {
            gwatReceiverBase: totalBaseAmount,
            // Third call when unbundling from the source chain was set as cancelled, so no new funds will be credited
            // Since the latest snapshot is after the first unbundle, the expected balance increase is 0
            gwatReceiverToken: 0n
        });
    });
});
