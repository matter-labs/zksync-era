/**
 * This suite contains tests checking Interop-B bundles features,
 * those that were included as part of v31 protocol upgrade.
 *
 * This suite checks Asset Tracker accounting for Interop-B bundles (v31):
 * - Baseline GWAT/L2AssetTracker balances before sending.
 * - After sending on source (before GW batch): L2AssetTracker and GWAT unchanged.
 * - After gateway executes source batch: GWAT sender decreases (L2AssetTracker unchanged).
 * - After destination executes bundle (before GW batch): GWAT unchanged.
 * - After gateway executes destination batch: GWAT receiver increases only for executed calls.
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

describe('Interop-B behavior checks', () => {
    const ctx = new InteropTestContext();

    // Stored bundle data for cross-test assertions
    const interopRequests: Record<string, () => Promise<TransactionReceipt>> = {};
    const bundles: Record<string, { amounts: string[]; receipt?: TransactionReceipt }> = {};
    const messages: Record<string, { amount: string; receipt?: TransactionReceipt }> = {};
    const unbundling: Record<
        string,
        {
            receipt?: TransactionReceipt;
            amounts?: { baseAmount: string; tokenAmount: string };
            data?: Output;
        }
    > = {};

    const bundleExecutionBlock: number[] = [];
    let trackerSnapshotBefore: AssetTrackerBalanceSnapshot;
    let trackerSnapshotAfterSend: AssetTrackerBalanceSnapshot;
    let totalBaseAmount = { bundles: 0n, messages: 0n, unbundling: 0n };
    let totalTokenAmount = { bundles: 0n, messages: 0n, unbundling: 0n };
    let totalMsgValue = { bundles: 0n, messages: 0n, unbundling: 0n };
    let totalFeePaid = { bundles: 0n, messages: 0n, unbundling: 0n };

    // Failing call is some random call to a contract that doesn't exist
    const failingCallContract = '0x000000000000000000000000000000000000feed';
    const failingCallCalldata = '0x00056d83'; // selector: "someVeryUnfortunateCall()"

    // Final call status after unbundling
    const finalCallStatusesFromDestination = [CallStatus.Executed, CallStatus.Cancelled, CallStatus.Executed];
    const finalCallStatusesFromSource = [CallStatus.Executed, CallStatus.Unprocessed, CallStatus.Cancelled];

    // We send all test bundles at once to make testing faster.
    // This way, we don't have to wait for each bundle to reach Chain B separately.
    beforeAll(async () => {
        if (ctx.skipInteropTests) return;
        await ctx.initialize(__filename);

        await ctx.performSharedSetup();

        await (await ctx.interop1TokenA.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, MaxUint256)).wait();
        // Take a snapshot of asset tracker balances before sending any bundles or messages
        trackerSnapshotBefore = await ctx.snapshotInteropAssetTrackerBalances();

        let nextNonce = await ctx.interop1Wallet.getNonce();
        const withNonce = (overrides: Overrides = {}): Overrides => ({
            ...overrides,
            nonce: nextNonce++
        });

        ///////////// SENDING BUNDLES /////////////
        // SINGLE DIRECT CALL BUNDLE
        // Simple transfer
        {
            const amount = ctx.getTransferAmount();

            const msgValue = ctx.isSameBaseToken ? amount : 0n;
            totalMsgValue.bundles += msgValue;
            interopRequests.singleDirect = async () => {
                return ctx.fromInterop1RequestInterop(
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
            };

            bundles.singleDirect = { amounts: [amount.toString()] };
            totalBaseAmount.bundles += amount;
        }

        // SINGLE INDIRECT CALL BUNDLE
        // Simple token transfer
        {
            const amount = await ctx.getTransferAmount();

            interopRequests.singleIndirect = async () => {
                return ctx.fromInterop1RequestInterop(
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
            };

            bundles.singleIndirect = { amounts: [amount.toString()] };
            totalTokenAmount.bundles += amount;
        }

        // TWO DIRECT CALLS BUNDLE
        // Transfer to two different recipients
        {
            const baseAmountA = ctx.getTransferAmount();
            const baseAmountB = ctx.getTransferAmount();
            const totalAmount = baseAmountA + baseAmountB;

            const msgValue = ctx.isSameBaseToken ? totalAmount : 0n;
            totalMsgValue.bundles += msgValue;
            interopRequests.twoDirect = async () => {
                return ctx.fromInterop1RequestInterop(
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
            };

            bundles.twoDirect = { amounts: [baseAmountA.toString(), baseAmountB.toString()] };
            totalBaseAmount.bundles += totalAmount;
        }

        // TWO INDIRECT CALLS BUNDLE
        // Two token transfers to different recipients
        {
            const tokenAmountA = await ctx.getTransferAmount();
            const tokenAmountB = await ctx.getTransferAmount();
            const totalAmount = tokenAmountA + tokenAmountB;

            interopRequests.twoIndirect = async () => {
                return ctx.fromInterop1RequestInterop(
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
            };

            bundles.twoIndirect = { amounts: [tokenAmountA.toString(), tokenAmountB.toString()] };
            totalTokenAmount.bundles += totalAmount;
        }

        // MIXED BUNDLE
        // One transfer and one token transfer
        {
            const baseAmount = ctx.getTransferAmount();
            const tokenAmount = await ctx.getTransferAmount();

            const msgValue = ctx.isSameBaseToken ? baseAmount : 0n;
            totalMsgValue.bundles += msgValue;
            interopRequests.mixed = async () => {
                return ctx.fromInterop1RequestInterop(
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
            };

            bundles.mixed = { amounts: [baseAmount.toString(), tokenAmount.toString()] };
            totalBaseAmount.bundles += baseAmount;
            totalTokenAmount.bundles += tokenAmount;
        }

        ///////////// SENDING MESSAGES /////////////
        const recipient = formatEvmV1Address(ctx.dummyInteropRecipient, ctx.interop2ChainId);
        const assetRouterRecipient = formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS, ctx.interop2ChainId);
        // SENDING A BASE TOKEN MESSAGE
        {
            const amount = ctx.getTransferAmount();

            const msgValue = ctx.isSameBaseToken ? amount : 0n;
            totalMsgValue.messages += msgValue;
            interopRequests.baseToken = async () => {
                return ctx.interop1InteropCenter
                    .sendMessage(recipient, '0x', await ctx.directCallAttrs(amount), withNonce({ value: msgValue }))
                    .then((tx) => tx.wait());
            };

            messages.baseToken = { amount: amount.toString() };
            totalBaseAmount.messages += amount;
        }

        // SENDING A NATIVE ERC20 TOKEN MESSAGE
        // TODO: make it actually send a native ERC20 token
        // {
        //     const amount = await ctx.getAndApproveTokenTransferAmount();
        //     totalTokenAmount += amount;

        //     interopRequests.nativeERC20 = ctx.interop1InteropCenter
        //         .sendMessage(
        //             assetRouterRecipient,
        //             ctx.getTokenTransferSecondBridgeData(ctx.tokenA.assetId!, amount, ctx.interop2Recipient.address),
        //             await ctx.indirectCallAttrs(),
        //             withNonce()
        //         )
        //         .then((tx) => tx.wait());

        //     messages.nativeERC20 = { amount: amount.toString() };
        // }

        // SENDING INTEROP1'S BASE TOKEN MESSAGE
        // TODO: adapt checks for this to work
        // This test only makes sense when chains have DIFFERENT base tokens
        // The case where chains have the same base token is already tested above: `SENDING A BASE TOKEN MESSAGE`
        if (!ctx.isSameBaseToken) {
            const amount = ctx.getTransferAmount();
            totalMsgValue.messages += amount;

            interopRequests.interop1BaseToken = async () => {
                return ctx.interop1InteropCenter
                    .sendMessage(
                        assetRouterRecipient,
                        ctx.getTokenTransferSecondBridgeData(
                            ctx.baseToken1.assetId!,
                            amount,
                            ctx.interop2Recipient.address
                        ),
                        await ctx.indirectCallAttrs(amount),
                        withNonce({ value: amount })
                    )
                    .then((tx) => tx.wait());
            };

            messages.interop1BaseToken = { amount: amount.toString() };
            totalBaseAmount.messages += amount;
        }

        // SENDING A BRIDGED ERC20 TOKEN MESSAGE
        {
            const amount = await ctx.getTransferAmount();
            totalTokenAmount.messages += amount;

            interopRequests.bridgedERC20 = async () => {
                return ctx.interop1InteropCenter
                    .sendMessage(
                        assetRouterRecipient,
                        ctx.getTokenTransferSecondBridgeData(
                            ctx.tokenA.assetId!,
                            amount,
                            ctx.interop2Recipient.address
                        ),
                        await ctx.indirectCallAttrs(),
                        withNonce()
                    )
                    .then((tx) => tx.wait());
            };

            messages.bridgedERC20 = { amount: amount.toString() };
        }

        ///////////// SENDING BUNDLES THAT NEED UNBUNDLING /////////////
        // FAILING BUNDLE THAT WILL BE UNBUNDLED FROM THE DESTINATION CHAIN
        // One transfer, one failing call, and one token transfer
        // Additionally sets a specific unbundler address
        {
            const baseAmount = ctx.getTransferAmount();
            const tokenAmount = await ctx.getTransferAmount();
            totalBaseAmount.unbundling += baseAmount;
            totalTokenAmount.unbundling += tokenAmount;

            const msgValue = ctx.isSameBaseToken ? baseAmount : 0n;
            totalMsgValue.unbundling += msgValue;
            interopRequests.fromDestinationChain = async () => {
                return await ctx.fromInterop1RequestInterop(
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
                    {
                        executionAddress: ctx.interop2RichWallet.address,
                        unbundlerAddress: ctx.interop2RichWallet.address
                    },
                    withNonce({ value: msgValue })
                );
            };

            unbundling.fromDestinationChain = {
                amounts: { baseAmount: baseAmount.toString(), tokenAmount: tokenAmount.toString() }
            };
        }

        // FAILING BUNDLE THAT WILL BE UNBUNDLED FROM THE SOURCE CHAIN
        // One transfer, one failing call, and one token transfer
        {
            const baseAmount = ctx.getTransferAmount();
            const tokenAmount = await ctx.getTransferAmount();
            totalBaseAmount.unbundling += baseAmount;
            totalTokenAmount.unbundling += tokenAmount;

            const msgValue = ctx.isSameBaseToken ? baseAmount : 0n;
            totalMsgValue.unbundling += msgValue;
            interopRequests.fromSourceChain = async () => {
                return await ctx.fromInterop1RequestInterop(
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
            };

            unbundling.fromSourceChain = {
                amounts: { baseAmount: baseAmount.toString(), tokenAmount: tokenAmount.toString() }
            };
        }
    });

    test('Can send bundles that need unbundling', async () => {
        if (ctx.skipInteropTests) return;

        const balanceBefore = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
        const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

        // Send all bundles at once to make testing faster
        for (const bundleToUnbundleName of Object.keys(unbundling)) {
            const receipt = await interopRequests[bundleToUnbundleName]();
            totalFeePaid.unbundling += BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
            unbundling[bundleToUnbundleName].receipt = receipt;
        }

        // Asset tracker state does not change until the batches containing the bundles are executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotBefore, {});

        // Assert the balances after sending the bundles are as expected
        const balanceAfter = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
        if (ctx.isSameBaseToken) {
            expect(balanceAfter.toString()).toBe(
                (balanceBefore - totalMsgValue.unbundling - totalFeePaid.unbundling).toString()
            );
        } else {
            expect(balanceAfter.toString()).toBe((balanceBefore - totalBaseAmount.unbundling).toString());
        }
        const tokenBalanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect(tokenBalanceAfter.toString()).toBe((tokenBalanceBefore - totalTokenAmount.unbundling).toString());
    });

    test('Can send bundles', async () => {
        if (ctx.skipInteropTests) return;

        const balanceBefore = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
        const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

        // Send all bundles at once to make testing faster
        for (const bundleName of Object.keys(bundles)) {
            const receipt = await interopRequests[bundleName]();
            totalFeePaid.bundles += BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
            bundles[bundleName].receipt = receipt;
        }

        // Asset tracker state does not change until the batches containing the bundles are executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotBefore, {});

        // Assert the balances after sending the bundles are as expected
        const balanceAfter = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
        if (ctx.isSameBaseToken) {
            expect(balanceAfter.toString()).toBe(
                (balanceBefore - totalMsgValue.bundles - totalFeePaid.bundles).toString()
            );
        } else {
            expect(balanceAfter.toString()).toBe((balanceBefore - totalBaseAmount.bundles).toString());
        }
        const tokenBalanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect(tokenBalanceAfter.toString()).toBe((tokenBalanceBefore - totalTokenAmount.bundles).toString());
    });

    test('Can send messages', async () => {
        if (ctx.skipInteropTests) return;

        const nativeBalanceBefore = await ctx.interop1Wallet.getBalance();
        const baseToken2BalanceBefore = ctx.isSameBaseToken
            ? 0n
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
        const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

        // Send all messages at once to make testing faster
        for (const messageName of Object.keys(messages)) {
            const receipt = await interopRequests[messageName]();
            totalFeePaid.messages += BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
            messages[messageName].receipt = receipt;
        }

        // Asset tracker state does not change until the batches containing the messages are executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotBefore, {});

        // Assert the balances after sending the messages are as expected
        const nativeBalanceAfter = await ctx.interop1Wallet.getBalance();
        expect(nativeBalanceAfter.toString()).toBe(
            (nativeBalanceBefore - totalMsgValue.messages - totalFeePaid.messages).toString()
        );
        if (!ctx.isSameBaseToken) {
            // TODO: adapt checks for this to work
            const baseToken2BalanceAfter = await ctx.getTokenBalance(
                ctx.interop1Wallet,
                ctx.baseToken2.l2AddressSecondChain!
            );
            expect(baseToken2BalanceAfter.toString()).toBe(
                (baseToken2BalanceBefore - totalBaseAmount.messages).toString()
            );
        }
        const tokenBalanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect(tokenBalanceAfter.toString()).toBe((tokenBalanceBefore - totalTokenAmount.messages).toString());
    });

    test('Can send unbundling bundle from the source chain', async () => {
        // We need to wait for bundle data to be available
        await waitForL2ToL1LogProof(
            ctx.interop1Wallet,
            unbundling.fromSourceChain.receipt!.blockNumber,
            unbundling.fromSourceChain.receipt!.hash
        );

        // Store the bundle data
        unbundling.fromDestinationChain.data = await getInteropBundleData(
            ctx.interop1Provider,
            unbundling.fromDestinationChain.receipt!.hash,
            0
        );
        unbundling.fromSourceChain.data = await getInteropBundleData(
            ctx.interop1Provider,
            unbundling.fromSourceChain.receipt!.hash,
            0
        );

        // UNBUNDLING BUNDLE FROM THE SOURCE CHAIN
        // We send another bundle from the source chain, verifying the failing bundle and then unbundling it
        // We send this bundle directly to make testing faster.
        {
            const verifyBundleData = ctx.interop2InteropHandler.interface.encodeFunctionData('verifyBundle', [
                unbundling.fromSourceChain.data!.rawData,
                unbundling.fromSourceChain.data!.proofDecoded
            ]);
            const unbundleBundleData = ctx.interop2InteropHandler.interface.encodeFunctionData('unbundleBundle', [
                ctx.interop1ChainId,
                unbundling.fromSourceChain.data!.rawData,
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

            unbundling.unbundlingBundleReceipt = { receipt };
        }
    });

    test('GWAssetTracker balances after batch execution reflect sent bundles and messages', async () => {
        // We wait for the last of these bundles to be executable on the destination chain.
        // By then, all of the bundles should be executable.
        await ctx.awaitInteropBundle(unbundling.fromSourceChain.receipt!.hash);
        // Assert the asset tracker state changes after interop bundles are executed on GW, debiting the sender chain
        await ctx.assertAssetTrackerBalances(trackerSnapshotBefore, {
            gwatSenderBase: -Object.values(totalBaseAmount).reduce((a, b) => a + b, 0n),
            gwatSenderToken: -Object.values(totalTokenAmount).reduce((a, b) => a + b, 0n)
        });
        // Update the snapshot after the bundles are executed on GW.
        trackerSnapshotAfterSend = await ctx.snapshotInteropAssetTrackerBalances();
    });

    test('Can receive a single direct call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(bundles.singleDirect.receipt!.hash);
        bundleExecutionBlock.push(execReceipt!.blockNumber);

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
        bundleExecutionBlock.push(execReceipt!.blockNumber);
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
        bundleExecutionBlock.push(execReceipt!.blockNumber);

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
        bundleExecutionBlock.push(execReceipt!.blockNumber);

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
        bundleExecutionBlock.push(execReceipt!.blockNumber);

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

    test('Can receive a message sending a base token', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(messages.baseToken.receipt!.hash);
        bundleExecutionBlock.push(execReceipt!.blockNumber);

        // Check the dummy interop recipient balance increased by the interop call value
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(messages.baseToken.amount);

        // Asset tracker state does not change until the batch containing the execution of the message is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    // test('Can receive a message sending a native ERC20 token', async () => {
    //     if (ctx.skipInteropTests) return;

    //     const execReceipt = await ctx.readAndBroadcastInteropBundle(messages.nativeERC20.receipt!.hash);
    //     bundleExecutionBlock.push(execReceipt!.blockNumber);
    //     ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

    //     // The balance before is 0 as the token did not yet exist on the second chain.
    //     const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
    //     expect(recipientBalance.toString()).toBe(messages.nativeERC20.amount);
    //     await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    // });

    test('Can receive a message sending the base token from the sending chain', async () => {
        // This test only makes sense when chains have DIFFERENT base tokens
        if (ctx.skipInteropTests || ctx.isSameBaseToken) return;

        const execReceipt = await ctx.readAndBroadcastInteropBundle(messages.interop1BaseToken.receipt!.hash);
        bundleExecutionBlock.push(execReceipt!.blockNumber);
        ctx.baseToken1.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.baseToken1.assetId);

        // The balance before is 0 as the token did not yet exist on the second chain.
        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.baseToken1.l2AddressSecondChain!);
        expect(recipientBalance.toString()).toBe(messages.interop1BaseToken.amount);

        // Asset tracker state does not change until the batch containing the execution of the message is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    test('Can receive a message sending a bridged token', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );

        const execReceipt = await ctx.readAndBroadcastInteropBundle(messages.bridgedERC20.receipt!.hash);
        bundleExecutionBlock.push(execReceipt!.blockNumber);
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(messages.bridgedERC20.amount);

        // Asset tracker state does not change until the batch containing the execution of the message is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    test('Cannot unbundle a non-verified bundle', async () => {
        if (ctx.skipInteropTests) return;

        await expect(
            ctx.interop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                unbundling.fromDestinationChain.data!.rawData,
                [CallStatus.Executed, CallStatus.Cancelled, CallStatus.Executed]
            )
        ).rejects.toThrow();
    });

    test('Can verify a bundle', async () => {
        if (ctx.skipInteropTests) return;

        // Try to execute atomically - should revert
        await expect(
            ctx.readAndBroadcastInteropBundle(unbundling.fromDestinationChain.receipt!.hash)
        ).rejects.toThrow();

        // Bundle is still Unreceived, so verify it so it's ready for unbundling
        const receipt = await ctx.interop2InteropHandler.verifyBundle(
            unbundling.fromDestinationChain.data!.rawData,
            unbundling.fromDestinationChain.data!.proofDecoded
        );
        await receipt.wait();

        // Bundle is now marked as verified
        expect(
            (await ctx.interop2InteropHandler.bundleStatus(unbundling.fromDestinationChain.data!.bundleHash)).toString()
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
            altInterop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                unbundling.fromDestinationChain.data!.rawData,
                [CallStatus.Executed, CallStatus.Unprocessed, CallStatus.Executed]
            )
        ).rejects.toThrow();
    });

    test('Cannot unbundle a failing call', async () => {
        if (ctx.skipInteropTests) return;
        await expect(
            ctx.interop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                unbundling.fromDestinationChain.data!.rawData,
                [CallStatus.Unprocessed, CallStatus.Executed, CallStatus.Unprocessed]
            )
        ).rejects.toThrow();
    });

    test('Can unbundle from the destination chain', async () => {
        if (ctx.skipInteropTests) return;
        const bundleHash = unbundling.fromDestinationChain.data!.bundleHash;

        // PROGRESSIVE UNBUNDLING
        // Leave call 0 as unprocessed (base token transfer), cancel call 1, and execute call 2 (token transfer)
        const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        const firstCallStatuses = [CallStatus.Unprocessed, CallStatus.Cancelled, CallStatus.Executed];
        const firstUnbundleReceipt = await (
            await ctx.interop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                unbundling.fromDestinationChain.data!.rawData,
                firstCallStatuses
            )
        ).wait();
        bundleExecutionBlock.push(firstUnbundleReceipt.blockNumber);
        // Balance checks
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);
        const tokenBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect((tokenBalance - tokenBalanceBefore).toString()).toBe(
            unbundling.fromDestinationChain.amounts!.tokenAmount
        );
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
            gwatReceiverToken: BigInt(unbundling.fromDestinationChain.amounts!.tokenAmount)
        });
        // Take new snapshot
        trackerSnapshotAfterSend = await ctx.snapshotInteropAssetTrackerBalances();

        // PROGRESSIVE UNBUNDLING
        // Unbundle again and process call 0 (base token transfer)
        const balanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        const secondUnbundleReceipt = await (
            await ctx.interop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                unbundling.fromDestinationChain.data!.rawData,
                [CallStatus.Executed, CallStatus.Unprocessed, CallStatus.Unprocessed]
            )
        ).wait();
        bundleExecutionBlock.push(secondUnbundleReceipt.blockNumber);
        // Balance checks
        const balance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((balance - balanceBefore).toString()).toBe(unbundling.fromDestinationChain.amounts!.baseAmount);
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
            ctx.interop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                unbundling.fromDestinationChain.data!.rawData,
                [CallStatus.Executed, CallStatus.Cancelled, CallStatus.Executed]
            )
        ).rejects.toThrow();
    });

    test('Cannot unbundle a cancelled call', async () => {
        if (ctx.skipInteropTests) return;
        await expect(
            ctx.interop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                unbundling.fromDestinationChain.data!.rawData,
                [CallStatus.Unprocessed, CallStatus.Executed, CallStatus.Unprocessed]
            )
        ).rejects.toThrow();
    });

    test('Cannot cancel a processed call', async () => {
        if (ctx.skipInteropTests) return;
        await expect(
            ctx.interop2InteropHandler.unbundleBundle(
                ctx.interop1ChainId,
                unbundling.fromDestinationChain.data!.rawData,
                [CallStatus.Cancelled, CallStatus.Cancelled, CallStatus.Cancelled]
            )
        ).rejects.toThrow();
    });

    test('Can send an unbundling bundle from the source chain', async () => {
        if (ctx.skipInteropTests) return;

        // We need to wait for the bundle to be executable on the destination chain.
        await ctx.awaitInteropBundle(unbundling.unbundlingBundleReceipt.receipt!.hash);

        const bundleHash = unbundling.fromSourceChain.data!.bundleHash;

        const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        const balanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast unbundling bundle from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(unbundling.unbundlingBundleReceipt.receipt!.hash);
        bundleExecutionBlock.push(execReceipt!.blockNumber);

        // Balance checks
        const tokenBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        // Third call was set as cancelled, so no funds will be credited
        expect((tokenBalance - tokenBalanceBefore).toString()).toBe('0');
        const balance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((balance - balanceBefore).toString()).toBe(unbundling.fromSourceChain.amounts!.baseAmount);
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

    test('Gateway balances after batch execution reflect executed bundles', async () => {
        if (ctx.skipInteropTests) return;

        const maxExecutedBlock = Math.max(...bundleExecutionBlock);
        await ctx.waitUntilInterop2BlockExecutedOnGateway(maxExecutedBlock);

        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend, {
            gwatReceiverBase: Object.values(totalBaseAmount).reduce((a, b) => a + b, 0n),
            gwatReceiverToken: Object.values(totalTokenAmount).reduce((a, b) => a + b, 0n)
        });
    });

    afterAll(async () => {
        await ctx.deinitialize();
    });
});
