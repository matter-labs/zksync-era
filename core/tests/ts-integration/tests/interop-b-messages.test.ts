/**
 * This suite contains tests checking Interop-B message features,
 * those that were included as part of v31 protocol upgrade.
 */

import { InteropTestContext } from '../src/interop-setup';
import { formatEvmV1Address } from '../src/helpers';
import { L2_ASSET_ROUTER_ADDRESS } from '../src/constants';
import { TransactionReceipt } from 'ethers';

describe('Interop-B Messages behavior checks', () => {
    const ctx = new InteropTestContext();

    // Stored message data for cross-test assertions
    const messages: Record<string, { amount: string; receipt: TransactionReceipt }> = {};

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

        // SENDING A BASE TOKEN MESSAGE
        {
            const amount = ctx.getTransferAmount();
            const balanceBefore = ctx.isSameBaseToken
                ? await ctx.interop1Wallet.getBalance()
                : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);

            const tx = await ctx.interop1InteropCenter.sendMessage(recipient, '0x', await ctx.directCallAttrs(amount), {
                value: ctx.isSameBaseToken ? amount : 0n
            });
            const receipt = await tx.wait();

            const balanceAfter = ctx.isSameBaseToken
                ? await ctx.interop1Wallet.getBalance()
                : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);

            if (ctx.isSameBaseToken) {
                const feePaid = BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
                expect((balanceAfter + feePaid).toString()).toBe((balanceBefore - amount).toString());
            } else {
                expect((balanceAfter - balanceBefore).toString()).toBe((-amount).toString());
            }

            messages.baseToken = { amount: amount.toString(), receipt };
        }

        // SENDING A NATIVE ERC20 TOKEN MESSAGE
        {
            const amount = await ctx.getAndApproveTokenTransferAmount();
            const balanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

            const tx = await ctx.interop1InteropCenter.sendMessage(
                assetRouterRecipient,
                ctx.getTokenTransferSecondBridgeData(ctx.tokenA.assetId!, amount, ctx.interop2Recipient.address),
                await ctx.indirectCallAttrs()
            );
            const receipt = await tx.wait();

            const balanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
            expect((balanceAfter - balanceBefore).toString()).toBe((-amount).toString());

            messages.nativeERC20 = { amount: amount.toString(), receipt };
        }

        // SENDING INTEROP1'S BASE TOKEN MESSAGE
        // This test only makes sense when chains have DIFFERENT base tokens
        // The case where chains have the same base token is already tested above: `SENDING A BASE TOKEN MESSAGE`
        if (!ctx.isSameBaseToken) {
            const amount = ctx.getTransferAmount();
            const balanceBefore = await ctx.interop1Wallet.getBalance();

            const tx = await ctx.interop1InteropCenter.sendMessage(
                assetRouterRecipient,
                ctx.getTokenTransferSecondBridgeData(ctx.baseToken1.assetId!, amount, ctx.interop2Recipient.address),
                await ctx.indirectCallAttrs(amount),
                { value: amount }
            );
            const receipt = await tx.wait();

            const balanceAfter = await ctx.interop1Wallet.getBalance();
            const feePaid = BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
            expect(balanceAfter + feePaid).toBe(balanceBefore - amount);

            messages.interop1BaseToken = { amount: amount.toString(), receipt };
        }

        // SENDING A BRIDGED ERC20 TOKEN MESSAGE
        {
            const amount = await ctx.getAndApproveBridgedTokenTransferAmount();
            const balanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.bridgedToken.l2Address!);

            const tx = await ctx.interop1InteropCenter.sendMessage(
                assetRouterRecipient,
                ctx.getTokenTransferSecondBridgeData(ctx.bridgedToken.assetId!, amount, ctx.interop2Recipient.address),
                await ctx.indirectCallAttrs()
            );
            const receipt = await tx.wait();

            const balanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.bridgedToken.l2Address!);
            expect((balanceAfter - balanceBefore).toString()).toBe((-amount).toString());

            messages.bridgedERC20 = { amount: amount.toString(), receipt };
        }

        // Wait for the last message to be executable on the receiver chain.
        // By then, all messages should be executable.
        await ctx.awaitInteropBundle(messages.bridgedERC20.receipt.hash);
    });

    test('Can receive a message sending a base token', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(messages.baseToken.receipt.hash);

        // Check the dummy interop recipient balance increased by the interop call value
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(messages.baseToken.amount);
    });

    test('Can receive a message sending a native ERC20 token', async () => {
        if (ctx.skipInteropTests) return;

        await ctx.readAndBroadcastInteropBundle(messages.nativeERC20.receipt.hash);
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

        // The balance before is 0 as the token did not yet exist on the second chain.
        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect(recipientBalance.toString()).toBe(messages.nativeERC20.amount);
    });

    test('Can receive a message sending the base token from the sending chain', async () => {
        // This test only makes sense when chains have DIFFERENT base tokens
        if (ctx.skipInteropTests || ctx.isSameBaseToken) return;

        await ctx.readAndBroadcastInteropBundle(messages.interop1BaseToken.receipt.hash);
        ctx.baseToken1.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.baseToken1.assetId);

        // The balance before is 0 as the token did not yet exist on the second chain.
        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.baseToken1.l2AddressSecondChain!);
        expect(recipientBalance.toString()).toBe(messages.interop1BaseToken.amount);
    });

    test('Can receive a message sending a bridged token', async () => {
        if (ctx.skipInteropTests) return;

        await ctx.readAndBroadcastInteropBundle(messages.bridgedERC20.receipt.hash);
        ctx.bridgedToken.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(
            ctx.bridgedToken.assetId
        );

        // The balance before is 0 as the token did not yet exist on the second chain.
        const recipientBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.bridgedToken.l2AddressSecondChain!
        );
        expect(recipientBalance.toString()).toBe(messages.bridgedERC20.amount);
    });

    afterAll(async () => {
        await ctx.deinitialize();
    });
});
