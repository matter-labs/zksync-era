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

    let baseTokenAmount: bigint;
    let baseTokenReceipt: TransactionReceipt;

    let nativeERC20TokenAmount: bigint;
    let nativeERC20TokenReceipt: TransactionReceipt;

    let bridgedERC20TokenAmount: bigint;
    let bridgedERC20TokenReceipt: TransactionReceipt;

    let interop1BaseTokenAmount: bigint;
    let interop1BaseTokenReceipt: TransactionReceipt;

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

        // SENDING A BASE TOKEN MESSAGE
        baseTokenAmount = ctx.getTransferAmount();
        const senderBalanceBefore = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.assetId!);

        const baseTokenAttributes = [
            await ctx.erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [baseTokenAmount]),
            await ctx.erc7786AttributeDummy.interface.encodeFunctionData('executionAddress', [
                formatEvmV1Address(ctx.interop2RichWallet.address, ctx.interop2ChainId)
            ])
        ];

        const baseTokenTx = await ctx.interop1InteropCenter.sendMessage(recipient, '0x', baseTokenAttributes, {
            value: ctx.isSameBaseToken ? baseTokenAmount : 0n
        });
        baseTokenReceipt = await baseTokenTx.wait();

        if (ctx.isSameBaseToken) {
            const senderBalance = await ctx.interop1Wallet.getBalance();
            const feePaid = BigInt(baseTokenReceipt.gasUsed) * BigInt(baseTokenReceipt.gasPrice);
            expect((senderBalance + feePaid).toString()).toBe((senderBalanceBefore - baseTokenAmount).toString());
        } else {
            const senderBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2Address!);
            expect((senderBalance - senderBalanceBefore).toString()).toBe((-baseTokenAmount).toString());
        }

        // SEDNING A NATIVE ERC20 TOKEN MESSAGE
        nativeERC20TokenAmount = await ctx.getAndApproveTokenTransferAmount();
        const senderTokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

        const nativeERC20TokenAttributes = [
            await ctx.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0]),
            await ctx.erc7786AttributeDummy.interface.encodeFunctionData('executionAddress', [
                formatEvmV1Address(ctx.interop2RichWallet.address, ctx.interop2ChainId)
            ])
        ];

        const nativeERC20TokenTx = await ctx.interop1InteropCenter.sendMessage(
            formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS, ctx.interop2ChainId),
            ctx.getTokenTransferSecondBridgeData(
                ctx.tokenA.assetId!,
                nativeERC20TokenAmount,
                ctx.interop2Recipient.address
            ),
            nativeERC20TokenAttributes
        );
        nativeERC20TokenReceipt = await nativeERC20TokenTx.wait();

        const senderTokenBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect((senderTokenBalance - senderTokenBalanceBefore).toString()).toBe((-nativeERC20TokenAmount).toString());

        // SENDING INTEROP1'S BASE TOKEN MESSAGE
        // This test only makes sense when chains have DIFFERENT base tokens
        // The case where chains have the same base token is already tested above: `SENDING A BASE TOKEN MESSAGE`
        if (!ctx.isSameBaseToken) {
            interop1BaseTokenAmount = ctx.getTransferAmount();
            const senderInterop1BalanceBefore = await ctx.interop1Wallet.getBalance();

            const interop1BaseTokenAttributes = [
                await ctx.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [interop1BaseTokenAmount]),
                await ctx.erc7786AttributeDummy.interface.encodeFunctionData('executionAddress', [
                    formatEvmV1Address(ctx.interop2RichWallet.address, ctx.interop2ChainId)
                ])
            ];

            const interop1BaseTokenTx = await ctx.interop1InteropCenter.sendMessage(
                formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS, ctx.interop2ChainId),
                ctx.getTokenTransferSecondBridgeData(
                    ctx.baseToken1.assetId!,
                    interop1BaseTokenAmount,
                    ctx.interop2Recipient.address
                ),
                interop1BaseTokenAttributes,
                { value: interop1BaseTokenAmount }
            );
            interop1BaseTokenReceipt = await interop1BaseTokenTx.wait();

            const senderInterop1BaseBalance = await ctx.interop1Wallet.getBalance();
            const feePaid = BigInt(interop1BaseTokenReceipt.gasUsed) * BigInt(interop1BaseTokenReceipt.gasPrice);
            expect(senderInterop1BaseBalance + feePaid).toBe(senderInterop1BalanceBefore - interop1BaseTokenAmount);
        }

        // SENDING A BRIDGED ERC20 TOKEN MESSAGE
        bridgedERC20TokenAmount = await ctx.getAndApproveBridgedTokenTransferAmount();
        const senderBridgedTokenBalanceBefore = await ctx.getTokenBalance(
            ctx.interop1Wallet,
            ctx.bridgedToken.l2Address!
        );

        const bridgedERC20TokenAttributes = [
            await ctx.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0]),
            await ctx.erc7786AttributeDummy.interface.encodeFunctionData('executionAddress', [
                formatEvmV1Address(ctx.interop2RichWallet.address, ctx.interop2ChainId)
            ])
        ];

        const bridgedERC20TokenTx = await ctx.interop1InteropCenter.sendMessage(
            formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS, ctx.interop2ChainId),
            ctx.getTokenTransferSecondBridgeData(
                ctx.bridgedToken.assetId!,
                bridgedERC20TokenAmount,
                ctx.interop2Recipient.address
            ),
            bridgedERC20TokenAttributes
        );
        bridgedERC20TokenReceipt = await bridgedERC20TokenTx.wait();

        const senderBridgedTokenBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.bridgedToken.l2Address!);
        expect((senderBridgedTokenBalance - senderBridgedTokenBalanceBefore).toString()).toBe(
            (-bridgedERC20TokenAmount).toString()
        );

        // We wait for the last of these messages to be executable on the receiver chain.
        // By then, all of the messages should be executable.
        await ctx.awaitInteropBundle(bridgedERC20TokenReceipt.hash);
    });

    test('Can receive a message sending a base token', async () => {
        if (ctx.skipInteropTests) return;

        const recipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(baseTokenReceipt.hash);

        // Check the dummy interop recipient balance increased by the interop call value
        const recipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((recipientBalance - recipientBalanceBefore).toString()).toBe(baseTokenAmount.toString());
    });

    test('Can receive a message sending a native ERC20 token', async () => {
        if (ctx.skipInteropTests) return;

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(nativeERC20TokenReceipt.hash);
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

        // Check the token balance on the second chain was increased by the token transfer amount
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect(recipientTokenBalance.toString()).toBe(nativeERC20TokenAmount.toString());
    });

    test('Can receive a message sending the base token from the sending chain', async () => {
        if (ctx.skipInteropTests) return;
        // This test only makes sense when chains have DIFFERENT base tokens
        if (ctx.isSameBaseToken) return;

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(interop1BaseTokenReceipt.hash);
        ctx.baseToken1.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.baseToken1.assetId);

        // Check the token balance on the second chain was increased by the token transfer amount
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.baseToken1.l2AddressSecondChain!
        );
        expect(recipientTokenBalance.toString()).toBe(interop1BaseTokenAmount.toString());
    });

    test('Can receive a message sending a bridged token', async () => {
        if (ctx.skipInteropTests) return;

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(bridgedERC20TokenReceipt.hash);
        ctx.bridgedToken.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(
            ctx.bridgedToken.assetId
        );

        // Check the token balance on the second chain
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.bridgedToken.l2AddressSecondChain!
        );
        expect(recipientTokenBalance.toString()).toBe(bridgedERC20TokenAmount.toString());
    });

    afterAll(async () => {
        await ctx.deinitialize();
    });
});
