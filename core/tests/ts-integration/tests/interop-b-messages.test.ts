/**
 * This suite contains tests checking Interop-B message features,
 * those that were included as part of v31 protocol upgrade.
 *
 * This suite checks Asset Tracker accounting for Interop-B messages (v31):
 * - Baseline GWAT/L2AssetTracker balances before sending.
 * - After sending on source (before GW batch): L2AssetTracker and GWAT unchanged.
 * - After gateway executes source batch: GWAT sender decreases (L2AssetTracker unchanged).
 * - After destination executes message (before GW batch): GWAT unchanged.
 * - After gateway executes destination batch: GWAT receiver increases only for executed calls.
 */

import { AssetTrackerBalanceSnapshot, InteropTestContext } from '../src/interop-setup';
import { formatEvmV1Address } from '../src/helpers';
import { L2_ASSET_ROUTER_ADDRESS, L2_NATIVE_TOKEN_VAULT_ADDRESS } from '../src/constants';
import { MaxUint256, TransactionReceipt } from 'ethers';
import type { Overrides } from 'ethers';

describe('Interop-B Messages behavior checks', () => {
    const ctx = new InteropTestContext();

    // Stored message data for cross-test assertions
    const messages: Record<string, { amount: string; receipt?: TransactionReceipt }> = {};
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

    // We send all test messages at once to make testing faster.
    // This way, we don't have to wait for each message to reach Chain B separately.
    test('Can send cross chain messages', async () => {
        if (ctx.skipInteropTests) return;
        const recipient = formatEvmV1Address(ctx.dummyInteropRecipient, ctx.interop2ChainId);
        const assetRouterRecipient = formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS, ctx.interop2ChainId);

        const interopRequests: Record<string, Promise<TransactionReceipt>> = {};
        trackerSnapshotBefore = await ctx.snapshotInteropAssetTrackerBalances();

        await (await ctx.interop1TokenA.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, MaxUint256)).wait();
        const nativeBalanceBefore = await ctx.interop1Wallet.getBalance();
        const baseToken2BalanceBefore = ctx.isSameBaseToken
            ? 0n
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
        const tokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

        let nextNonce = await ctx.interop1Wallet.getNonce();
        const withNonce = (overrides: Overrides = {}): Overrides => ({
            ...overrides,
            nonce: nextNonce++
        });

        // SENDING A BASE TOKEN MESSAGE
        {
            const amount = ctx.getTransferAmount();

            const msgValue = ctx.isSameBaseToken ? amount : 0n;
            totalMsgValue += msgValue;
            interopRequests.baseToken = ctx.interop1InteropCenter
                .sendMessage(recipient, '0x', await ctx.directCallAttrs(amount), withNonce({ value: msgValue }))
                .then((tx) => tx.wait());

            messages.baseToken = { amount: amount.toString() };
            totalBaseAmount += amount;
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
            totalMsgValue += amount;

            interopRequests.interop1BaseToken = ctx.interop1InteropCenter
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

            messages.interop1BaseToken = { amount: amount.toString() };
            totalBaseAmount += amount;
        }

        // SENDING A BRIDGED ERC20 TOKEN MESSAGE
        {
            const amount = await ctx.getTransferAmount();
            totalTokenAmount += amount;

            interopRequests.bridgedERC20 = ctx.interop1InteropCenter
                .sendMessage(
                    assetRouterRecipient,
                    ctx.getTokenTransferSecondBridgeData(ctx.tokenA.assetId!, amount, ctx.interop2Recipient.address),
                    await ctx.indirectCallAttrs(),
                    withNonce()
                )
                .then((tx) => tx.wait());

            messages.bridgedERC20 = { amount: amount.toString() };
        }

        let totalFeePaid = 0n;
        // Send all messages at once to make testing faster
        for (const messageName of Object.keys(interopRequests)) {
            const receipt = await interopRequests[messageName];
            totalFeePaid += BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
            messages[messageName].receipt = receipt;
        }

        // Asset tracker state does not change until the batches containing the messages are executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotBefore, {});

        // Assert the balances after sending the messages are as expected
        const nativeBalanceAfter = await ctx.interop1Wallet.getBalance();
        expect(nativeBalanceAfter.toString()).toBe((nativeBalanceBefore - totalMsgValue - totalFeePaid).toString());
        if (!ctx.isSameBaseToken) {
            // TODO: adapt checks for this to work
            const baseToken2BalanceAfter = await ctx.getTokenBalance(
                ctx.interop1Wallet,
                ctx.baseToken2.l2AddressSecondChain!
            );
            expect((baseToken2BalanceAfter - baseToken2BalanceBefore).toString()).toBe((-totalBaseAmount).toString());
        }
        const tokenBalanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect((tokenBalanceAfter - tokenBalanceBefore).toString()).toBe((-totalTokenAmount).toString());

        // We wait for the last of these messages to be executable on the destination chain.
        // By then, all of the messages should be executable.
        await ctx.awaitInteropBundle(messages.bridgedERC20.receipt!.hash);
        // Assert the asset tracker state changes after interop messages are executed on GW, debiting the sender chain
        await ctx.assertAssetTrackerBalances(trackerSnapshotBefore, {
            gwatSenderBase: -totalBaseAmount,
            gwatSenderToken: -totalTokenAmount
        });
        // Update the snapshot after the messages are executed on GW.
        trackerSnapshotAfterSend = await ctx.snapshotInteropAssetTrackerBalances();
    });

    test('Can receive a message sending a base token', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        const execReceipt = await ctx.readAndBroadcastInteropBundle(messages.baseToken.receipt!.hash);
        executedInterop2Blocks.push(execReceipt!.blockNumber);

        // Check the dummy interop recipient balance increased by the interop call value
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(messages.baseToken.amount);

        // Asset tracker state does not change until the batch containing the execution of the message is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    // test('Can receive a message sending a native ERC20 token', async () => {
    //     if (ctx.skipInteropTests) return;

    //     const execReceipt = await ctx.readAndBroadcastInteropBundle(messages.nativeERC20.receipt!.hash);
    //     executedInterop2Blocks.push(execReceipt!.blockNumber);
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
        executedInterop2Blocks.push(execReceipt!.blockNumber);
        ctx.baseToken1.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.baseToken1.assetId);

        // The balance before is 0 as the token did not yet exist on the second chain.
        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.baseToken1.l2AddressSecondChain!);
        expect(recipientBalance.toString()).toBe(messages.interop1BaseToken.amount);

        // Asset tracker state does not change until the batch containing the execution of the message is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    test('Can receive a message sending a bridged token', async () => {
        if (ctx.skipInteropTests) return;

        const execReceipt = await ctx.readAndBroadcastInteropBundle(messages.bridgedERC20.receipt!.hash);
        executedInterop2Blocks.push(execReceipt!.blockNumber);
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

        // The balance before is 0 as the token did not yet exist on the second chain.
        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect(recipientBalance.toString()).toBe(messages.bridgedERC20.amount);

        // Asset tracker state does not change until the batch containing the execution of the message is executed on GW.
        await ctx.assertAssetTrackerBalances(trackerSnapshotAfterSend);
    });

    test('Gateway balances after batch execution reflect executed messages', async () => {
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
