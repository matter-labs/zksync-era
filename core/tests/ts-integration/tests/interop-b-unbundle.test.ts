/**
 * This suite contains tests checking Interop-B unbundle features,
 * those that were included as part of v32 protocol upgrade.
 */

import * as zksync from 'zksync-ethers';
import { InteropTestContext } from '../src/interop-setup';
import { formatEvmV1Address } from '../src/helpers';
import { ArtifactInteropHandler, L2_ASSET_ROUTER_ADDRESS, L2_INTEROP_HANDLER_ADDRESS } from '../src/constants';
import { TransactionReceipt } from 'ethers';
import { BundleStatus, CallStatus, getInteropBundleData, Output } from '../src/temp-sdk';

describe('Interop-B Unbundle behavior checks', () => {
    const ctx = new InteropTestContext();

    // Stored bundle data for cross-test assertions
    const bundles: Record<
        string,
        {
            receipt: TransactionReceipt;
            amounts?: { baseAmount: string; tokenAmount: string };
            data?: Output;
        }
    > = {};

    // Failing call is some random call to a contract that doesn't exist
    const failingCallContract = '0x000000000000000000000000000000000000feed';
    const failingCallCalldata = '0x00056d83'; // selector: "someVeryUnfortunateCall()"

    // Final call status after unbundling
    const finalCallStatuses = [CallStatus.Executed, CallStatus.Cancelled, CallStatus.Executed];

    beforeAll(async () => {
        await ctx.initialize(__filename);
        if (ctx.skipInteropTests) return;

        await ctx.performSharedSetup();
    });

    // We send all test bundles at once to make testing faster.
    // This way, we don't have to wait for each bundle to reach Chain B separately.
    test('Can send bundles that need unbundling', async () => {
        if (ctx.skipInteropTests) return;

        // FAILING BUNDLE THAT WILL BE UNBUNDLED FROM THE DESTINATION CHAIN
        // One transfer, one failing call, and one token transfer
        // Additionally sets a specific unbundler address
        {
            const baseAmount = ctx.getTransferAmount();
            const tokenAmount = await ctx.getAndApproveTokenTransferAmount();

            const balanceBefore = ctx.isSameBaseToken
                ? await ctx.interop1Wallet.getBalance()
                : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
            const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

            const msgValue = ctx.isSameBaseToken ? baseAmount : 0n;
            const receipt = await ctx.fromInterop1RequestInterop(
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
                { value: msgValue }
            );

            const balanceAfter = ctx.isSameBaseToken
                ? await ctx.interop1Wallet.getBalance()
                : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
            if (ctx.isSameBaseToken) {
                const feePaid = BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
                expect((balanceAfter + feePaid).toString()).toBe((balanceBefore - msgValue).toString());
            } else {
                expect((balanceAfter - balanceBefore).toString()).toBe((-baseAmount).toString());
            }
            const tokenBalanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
            expect((tokenBalanceAfter - tokenBalanceBefore).toString()).toBe((-tokenAmount).toString());

            bundles.fromDestinationChain = {
                receipt,
                amounts: { baseAmount: baseAmount.toString(), tokenAmount: tokenAmount.toString() }
            };
        }

        // FAILING BUNDLE THAT WILL BE UNBUNDLED FROM THE SOURCE CHAIN
        // One transfer, one failing call, and one token transfer
        {
            const baseAmount = ctx.getTransferAmount();
            const tokenAmount = await ctx.getAndApproveTokenTransferAmount();

            const balanceBefore = ctx.isSameBaseToken
                ? await ctx.interop1Wallet.getBalance()
                : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
            const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

            const msgValue = ctx.isSameBaseToken ? baseAmount : 0n;
            const receipt = await ctx.fromInterop1RequestInterop(
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
                { value: msgValue }
            );

            const balanceAfter = ctx.isSameBaseToken
                ? await ctx.interop1Wallet.getBalance()
                : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
            if (ctx.isSameBaseToken) {
                const feePaid = BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
                expect((balanceAfter + feePaid).toString()).toBe((balanceBefore - msgValue).toString());
            } else {
                expect((balanceAfter - balanceBefore).toString()).toBe((-baseAmount).toString());
            }
            const tokenBalanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
            expect((tokenBalanceAfter - tokenBalanceBefore).toString()).toBe((-tokenAmount).toString());

            bundles.fromSourceChain = {
                receipt,
                amounts: { baseAmount: baseAmount.toString(), tokenAmount: tokenAmount.toString() }
            };
        }

        // We wait for the last of these bundles to be executable on the destination chain.
        // By then, all of the bundles should be executable.
        await ctx.awaitInteropBundle(bundles.fromSourceChain.receipt.hash);

        // Store the bundle data
        bundles.fromDestinationChain.data = await getInteropBundleData(
            ctx.interop1Provider,
            bundles.fromDestinationChain.receipt.hash,
            0
        );
        bundles.fromSourceChain.data = await getInteropBundleData(
            ctx.interop1Provider,
            bundles.fromSourceChain.receipt.hash,
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
                finalCallStatuses
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
        await expect(ctx.readAndBroadcastInteropBundle(bundles.fromDestinationChain.receipt.hash)).rejects.toThrow();

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
        const firstUnbundleReceipt = await ctx.interop2InteropHandler.unbundleBundle(
            ctx.interop1ChainId,
            bundles.fromDestinationChain.data!.rawData,
            firstCallStatuses
        );
        await firstUnbundleReceipt.wait();
        // Balance checks
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);
        const tokenBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect(tokenBalance.toString()).toBe(bundles.fromDestinationChain.amounts!.tokenAmount);
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

        // PROGRESSIVE UNBUNDLING
        // Unbundle again and process call 0 (base token transfer)
        const balanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        const secondUnbundleReceipt = await ctx.interop2InteropHandler.unbundleBundle(
            ctx.interop1ChainId,
            bundles.fromDestinationChain.data!.rawData,
            [CallStatus.Executed, CallStatus.Unprocessed, CallStatus.Unprocessed]
        );
        await secondUnbundleReceipt.wait();
        // Balance checks
        const balance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((balance - balanceBefore).toString()).toBe(bundles.fromDestinationChain.amounts!.baseAmount);
        // Bundle remains Unbundled
        expect((await ctx.interop2InteropHandler.bundleStatus(bundleHash)).toString()).toBe(
            BundleStatus.Unbundled.toString()
        );
        // Calls were processed as specified
        for (const [index, callStatus] of finalCallStatuses.entries()) {
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
        await ctx.awaitInteropBundle(bundles.unbundlingBundleReceipt.receipt.hash);

        const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        const balanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast unbundling bundle from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(bundles.unbundlingBundleReceipt.receipt.hash);

        // Balance checks
        const tokenBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect((tokenBalance - tokenBalanceBefore).toString()).toBe(bundles.fromSourceChain.amounts!.tokenAmount);
        const balance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((balance - balanceBefore).toString()).toBe(bundles.fromSourceChain.amounts!.baseAmount);
        // Bundle is now Unbundled
        expect((await ctx.interop2InteropHandler.bundleStatus(bundleHash)).toString()).toBe(
            BundleStatus.Unbundled.toString()
        );
        // Calls were processed as specified
        for (const [index, callStatus] of finalCallStatuses.entries()) {
            expect((await ctx.interop2InteropHandler.callStatus(bundleHash, index)).toString()).toBe(
                callStatus.toString()
            );
        }
    });
});
