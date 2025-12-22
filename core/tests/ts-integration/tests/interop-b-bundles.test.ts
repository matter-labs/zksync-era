/**
 * This suite contains tests checking Interop-B bundles features,
 * those that were included as part of v31 protocol upgrade.
 */

import { InteropTestContext } from '../src/interop-setup';
import { formatEvmV1Address } from '../src/helpers';
import { L2_ASSET_ROUTER_ADDRESS } from '../src/constants';

describe('Interop-B bundles behavior checks', () => {
    const ctx = new InteropTestContext();

    beforeAll(async () => {
        await ctx.initialize(__filename);
        if (!ctx.skipInteropTests) {
            await ctx.performSharedSetup();
        }
    });

    test('Can perform cross chain transfer', async () => {
        if (ctx.skipInteropTests) return;
        const tokenTransferAmount = await ctx.getAndApproveTransferAmount();

        // Balances before transfer
        const senderTokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        const recipientTokenBalanceBefore = 0n;

        // Compose and send the interop request transaction
        const receipt = await ctx.fromInterop1RequestInterop(
            // Execution call starters for token transfer
            [
                {
                    to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                    data: ctx.getTokenTransferSecondBridgeData(
                        ctx.tokenA.assetId!,
                        tokenTransferAmount,
                        ctx.interop2Recipient.address
                    ),
                    callAttributes: [await ctx.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0])]
                }
            ],
            { executionAddress: ctx.interop2RichWallet.address }
        );

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(receipt.hash);

        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

        // Check the token balance on the first chain decreased by the token transfer amount
        const senderTokenBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect((senderTokenBalance - senderTokenBalanceBefore).toString()).toBe((-tokenTransferAmount).toString());
        // Check the token balance on the second chain was increased by the token transfer amount
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((recipientTokenBalance - recipientTokenBalanceBefore).toString()).toBe(tokenTransferAmount.toString());
    });

    test('Can perform cross chain bundle', async () => {
        if (ctx.skipInteropTests) return;
        const interopCallValue = BigInt(Math.floor(Math.random() * 900) + 100);
        const tokenTransferAmount = await ctx.getAndApproveTransferAmount();

        // Balances before interop
        const senderBalanceBefore = await ctx.interop1Wallet.getBalance();
        const senderTokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        const interopRecipientBalanceBefore = await ctx.getInterop2BalanceOfInterop1BaseToken(
            ctx.dummyInteropRecipient
        );
        const recipientTokenBalanceBefore = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );

        // Compose and send the interop request transaction
        const msgValue = ctx.isSameBaseToken ? interopCallValue : 0n;
        const receipt = await ctx.fromInterop1RequestInterop(
            [
                // Execution call starters for token transfer
                {
                    to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                    data: ctx.getTokenTransferSecondBridgeData(
                        ctx.tokenA.assetId!,
                        tokenTransferAmount,
                        ctx.interop2Recipient.address
                    ),
                    callAttributes: [await ctx.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0])]
                },
                // Execution call starters for interop call
                {
                    to: formatEvmV1Address(ctx.dummyInteropRecipient),
                    data: '0x',
                    callAttributes: [
                        await ctx.erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [
                            interopCallValue
                        ])
                    ]
                }
            ],
            { executionAddress: ctx.interop2RichWallet.address },
            { value: msgValue }
        );

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(receipt.hash);

        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

        // Check the sender balance decreased by the interop call value
        const senderBalance = await ctx.interop1Wallet.getBalance();
        const feePaid = BigInt(receipt.gasUsed) * BigInt(receipt.gasPrice);
        expect((senderBalance + feePaid).toString()).toBe((senderBalanceBefore - msgValue).toString());
        // Check the token balance on the first chain decreased by the token transfer amount
        const senderTokenBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect((senderTokenBalance - senderTokenBalanceBefore).toString()).toBe((-tokenTransferAmount).toString());

        // Check the dummy interop recipient balance increased by the interop call value
        const interopRecipientBalance = await ctx.getInterop2BalanceOfInterop1BaseToken(ctx.dummyInteropRecipient);
        expect((interopRecipientBalance - interopRecipientBalanceBefore).toString()).toBe(interopCallValue.toString());
        // Check the token balance on the second chain increased by the token transfer amount
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((recipientTokenBalance - recipientTokenBalanceBefore).toString()).toBe(tokenTransferAmount.toString());
    });

    afterAll(async () => {
        await ctx.deinitialize();
    });
});
