/**
 * This suite contains tests checking Interop-B bundles features,
 * those that were included as part of v31 protocol upgrade.
 */

import { InteropTestContext } from '../src/interop-setup';
import { formatEvmV1Address } from '../src/helpers';
import { L2_ASSET_ROUTER_ADDRESS, L2_NATIVE_TOKEN_VAULT_ADDRESS } from '../src/constants';
import { TransactionReceipt } from 'ethers';

describe('Interop-B Bundles behavior checks', () => {
    const ctx = new InteropTestContext();

    // Stored bundle data for cross-test assertions
    const bundles: Record<string, { amounts: string[]; receipt: TransactionReceipt }> = {};

    beforeAll(async () => {
        await ctx.initialize(__filename);
        if (ctx.skipInteropTests) return;

        await ctx.performSharedSetup();
    });

    // We send all test bundles at once to make testing faster.
    // This way, we don't have to wait for each bundle to reach Chain B separately.
    test('Can send bundles', async () => {
        if (ctx.skipInteropTests) return;

        // SINGLE DIRECT CALL BUNDLE
        // Simple transfer
        {
            const amount = ctx.getTransferAmount();
            const balanceBefore = ctx.isSameBaseToken
                ? await ctx.interop1Wallet.getBalance()
                : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);

            const msgValue = ctx.isSameBaseToken ? amount : 0n;
            const receipt = await ctx.fromInterop1RequestInterop(
                [
                    {
                        to: formatEvmV1Address(ctx.dummyInteropRecipient),
                        data: '0x',
                        callAttributes: [ctx.interopCallValueAttr(amount)]
                    }
                ],
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
                expect((balanceAfter - balanceBefore).toString()).toBe((-amount).toString());
            }

            bundles.singleDirect = { amounts: [amount.toString()], receipt };
        }

        // SINGLE INDIRECT CALL BUNDLE
        // Simple token transfer
        {
            const amount = await ctx.getAndApproveTokenTransferAmount();
            const balanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

            const receipt = await ctx.fromInterop1RequestInterop(
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
                { executionAddress: ctx.interop2RichWallet.address }
            );

            const balanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
            expect((balanceAfter - balanceBefore).toString()).toBe((-amount).toString());

            bundles.singleIndirect = { amounts: [amount.toString()], receipt };
        }

        // TWO DIRECT CALLS BUNDLE
        // Transfer to two different recipients
        {
            const baseAmountA = ctx.getTransferAmount();
            const baseAmountB = ctx.getTransferAmount();
            const totalAmount = baseAmountA + baseAmountB;
            const balanceBefore = ctx.isSameBaseToken
                ? await ctx.interop1Wallet.getBalance()
                : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);

            const msgValue = ctx.isSameBaseToken ? totalAmount : 0n;
            const receipt = await ctx.fromInterop1RequestInterop(
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
                { value: msgValue }
            );

            const balanceAfter = ctx.isSameBaseToken
                ? await ctx.interop1Wallet.getBalance()
                : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
            if (ctx.isSameBaseToken) {
                const feePaid = BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
                expect((balanceAfter + feePaid).toString()).toBe((balanceBefore - msgValue).toString());
            } else {
                expect((balanceAfter - balanceBefore).toString()).toBe((-totalAmount).toString());
            }

            bundles.twoDirect = {
                amounts: [baseAmountA.toString(), baseAmountB.toString()],
                receipt
            };
        }

        // TWO INDIRECT CALLS BUNDLE
        // Two token transfers to different recipients
        {
            const tokenAmountA = await ctx.getAndApproveTokenTransferAmount();
            const tokenAmountB = await ctx.getAndApproveTokenTransferAmount();
            const totalAmount = tokenAmountA + tokenAmountB;
            await (await ctx.interop1TokenA.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, totalAmount)).wait();
            const balanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

            const receipt = await ctx.fromInterop1RequestInterop(
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
                { executionAddress: ctx.interop2RichWallet.address }
            );

            const balanceAfter = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
            expect((balanceAfter - balanceBefore).toString()).toBe((-totalAmount).toString());

            bundles.twoIndirect = {
                amounts: [tokenAmountA.toString(), tokenAmountB.toString()],
                receipt
            };
        }

        // MIXED BUNDLE
        // One transfer and one token transfer
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

            bundles.mixed = {
                amounts: [baseAmount.toString(), tokenAmount.toString()],
                receipt
            };
        }

        // We wait for the last of these bundles to be executable on the receiver chain.
        // By then, all of the bundles should be executable.
        await ctx.awaitInteropBundle(bundles.mixed.receipt.hash);
    });

    test('Can receive a single direct call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(bundles.singleDirect.receipt.hash);

        // Check the dummy interop recipient balance increased by the interop call value
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(bundles.singleDirect.amounts[0]);
    });

    test('Can receive a single indirect call bundle', async () => {
        if (ctx.skipInteropTests) return;

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(bundles.singleIndirect.receipt.hash);
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

        // The balance before is 0 as the token did not yet exist on the second chain.
        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect(recipientBalance.toString()).toBe(bundles.singleIndirect.amounts[0]);
    });

    test('Can receive a two direct call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        const otherRecipientBalanceBefore = await ctx.getInterop2Balance(ctx.otherDummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(bundles.twoDirect.receipt.hash);

        // Check both recipients received their amounts
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(bundles.twoDirect.amounts[0]);
        const otherRecipientBalance = await ctx.getInterop2Balance(ctx.otherDummyInteropRecipient);
        expect((otherRecipientBalance - otherRecipientBalanceBefore).toString()).toBe(bundles.twoDirect.amounts[1]);
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
        await ctx.readAndBroadcastInteropBundle(bundles.twoIndirect.receipt.hash);

        // Check both recipients received their token amounts
        const recipientBalance = await ctx.getTokenBalance(ctx.interop2Recipient, ctx.tokenA.l2AddressSecondChain!);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(bundles.twoIndirect.amounts[0]);
        const otherRecipientBalance = await ctx.getTokenBalance(
            ctx.otherInterop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((otherRecipientBalance - otherRecipientBalanceBefore).toString()).toBe(bundles.twoIndirect.amounts[1]);
    });

    test('Can received a mixed call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        const recipientTokenBalanceBefore = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(bundles.mixed.receipt.hash);

        // Check the dummy interop recipient balance increased by the interop call value
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(bundles.mixed.amounts[0]);
        // Check the token balance on the second chain increased by the token transfer amount
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((recipientTokenBalance - recipientTokenBalanceBefore).toString()).toBe(bundles.mixed.amounts[1]);
    });

    afterAll(async () => {
        await ctx.deinitialize();
    });
});
