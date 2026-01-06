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

    let singleDirectAmount: bigint;
    let singleDirectReceipt: TransactionReceipt;

    let singleIndirectAmount: bigint;
    let singleIndirectReceipt: TransactionReceipt;

    let twoDirectAmount1: bigint;
    let twoDirectAmount2: bigint;
    let twoDirectReceipt: TransactionReceipt;

    let twoIndirectAmount1: bigint;
    let twoIndirectAmount2: bigint;
    let twoIndirectReceipt: TransactionReceipt;

    let mixedBundleBaseAmount: bigint;
    let mixedBundleTokenAmount: bigint;
    let mixedBundleReceipt: TransactionReceipt;

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
        singleDirectAmount = ctx.getTransferAmount();
        let senderBalanceBefore = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);

        let msgValue = ctx.isSameBaseToken ? singleDirectAmount : 0n;
        singleDirectReceipt = await ctx.fromInterop1RequestInterop(
            [
                // Execution call starters for interop call
                {
                    to: formatEvmV1Address(ctx.dummyInteropRecipient),
                    data: '0x',
                    callAttributes: [
                        await ctx.erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [
                            singleDirectAmount
                        ])
                    ]
                }
            ],
            { executionAddress: ctx.interop2RichWallet.address },
            { value: msgValue }
        );

        if (ctx.isSameBaseToken) {
            let senderBalance = await ctx.interop1Wallet.getBalance();
            let feePaid = BigInt(singleDirectReceipt.gasUsed) * BigInt(singleDirectReceipt.gasPrice);
            expect((senderBalance + feePaid).toString()).toBe((senderBalanceBefore - msgValue).toString());
        } else {
            let senderBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
            expect((senderBalance - senderBalanceBefore).toString()).toBe((-singleDirectAmount).toString());
        }

        // SINGLE INDIRECT CALL BUNDLE
        // Simple token transfer
        singleIndirectAmount = await ctx.getAndApproveTokenTransferAmount();
        let senderTokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

        singleIndirectReceipt = await ctx.fromInterop1RequestInterop(
            // Execution call starters for token transfer
            [
                {
                    to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                    data: ctx.getTokenTransferSecondBridgeData(
                        ctx.tokenA.assetId!,
                        singleIndirectAmount,
                        ctx.interop2Recipient.address
                    ),
                    callAttributes: [await ctx.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0])]
                }
            ],
            { executionAddress: ctx.interop2RichWallet.address }
        );

        let senderTokenBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect((senderTokenBalance - senderTokenBalanceBefore).toString()).toBe((-singleIndirectAmount).toString());

        // TWO DIRECT CALLS BUNDLE
        // Transfer to two different recipients
        twoDirectAmount1 = ctx.getTransferAmount();
        twoDirectAmount2 = ctx.getTransferAmount();
        senderBalanceBefore = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);

        let totalAmount = twoDirectAmount1 + twoDirectAmount2;
        msgValue = ctx.isSameBaseToken ? totalAmount : 0n;
        twoDirectReceipt = await ctx.fromInterop1RequestInterop(
            [
                // Execution call starters for interop call
                {
                    to: formatEvmV1Address(ctx.dummyInteropRecipient),
                    data: '0x',
                    callAttributes: [
                        await ctx.erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [
                            twoDirectAmount1
                        ])
                    ]
                },
                {
                    to: formatEvmV1Address(ctx.otherDummyInteropRecipient),
                    data: '0x',
                    callAttributes: [
                        await ctx.erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [
                            twoDirectAmount2
                        ])
                    ]
                }
            ],
            { executionAddress: ctx.interop2RichWallet.address },
            { value: msgValue }
        );

        if (ctx.isSameBaseToken) {
            let senderBalance = await ctx.interop1Wallet.getBalance();
            let feePaid = BigInt(twoDirectReceipt.gasUsed) * BigInt(twoDirectReceipt.gasPrice);
            expect((senderBalance + feePaid).toString()).toBe((senderBalanceBefore - msgValue).toString());
        } else {
            let senderBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
            expect((senderBalance - senderBalanceBefore).toString()).toBe((-totalAmount).toString());
        }

        // TWO INDIRECT CALLS BUNDLE
        // Two token transfers to different recipients
        twoIndirectAmount1 = await ctx.getAndApproveTokenTransferAmount();
        twoIndirectAmount2 = await ctx.getAndApproveTokenTransferAmount();
        const totalIndirectAmount = twoIndirectAmount1 + twoIndirectAmount2;
        await (await ctx.interop1TokenA.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, totalIndirectAmount)).wait();
        senderTokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

        twoIndirectReceipt = await ctx.fromInterop1RequestInterop(
            // Execution call starters for token transfer
            [
                {
                    to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                    data: ctx.getTokenTransferSecondBridgeData(
                        ctx.tokenA.assetId!,
                        twoIndirectAmount1,
                        ctx.interop2Recipient.address
                    ),
                    callAttributes: [await ctx.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0])]
                },
                {
                    to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                    data: ctx.getTokenTransferSecondBridgeData(
                        ctx.tokenA.assetId!,
                        twoIndirectAmount2,
                        ctx.otherInterop2Recipient.address
                    ),
                    callAttributes: [await ctx.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0])]
                }
            ],
            { executionAddress: ctx.interop2RichWallet.address }
        );

        senderTokenBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect((senderTokenBalance - senderTokenBalanceBefore).toString()).toBe((-totalIndirectAmount).toString());

        // MIXED BUNDLE
        // One transfer and one token transfer
        mixedBundleBaseAmount = ctx.getTransferAmount();
        mixedBundleTokenAmount = await ctx.getAndApproveTokenTransferAmount();

        senderBalanceBefore = ctx.isSameBaseToken
            ? await ctx.interop1Wallet.getBalance()
            : await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
        senderTokenBalanceBefore = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);

        msgValue = ctx.isSameBaseToken ? mixedBundleBaseAmount : 0n;
        mixedBundleReceipt = await ctx.fromInterop1RequestInterop(
            [
                // Execution call starters for token transfer
                {
                    to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                    data: ctx.getTokenTransferSecondBridgeData(
                        ctx.tokenA.assetId!,
                        mixedBundleTokenAmount,
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
                            mixedBundleBaseAmount
                        ])
                    ]
                }
            ],
            { executionAddress: ctx.interop2RichWallet.address },
            { value: msgValue }
        );

        if (ctx.isSameBaseToken) {
            let senderBalance = await ctx.interop1Wallet.getBalance();
            let feePaid = BigInt(mixedBundleReceipt.gasUsed) * BigInt(mixedBundleReceipt.gasPrice);
            expect((senderBalance + feePaid).toString()).toBe((senderBalanceBefore - msgValue).toString());
        } else {
            let senderBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.baseToken2.l2AddressSecondChain!);
            expect((senderBalance - senderBalanceBefore).toString()).toBe((-mixedBundleBaseAmount).toString());
        }
        senderTokenBalance = await ctx.getTokenBalance(ctx.interop1Wallet, ctx.tokenA.l2Address!);
        expect((senderTokenBalance - senderTokenBalanceBefore).toString()).toBe((-mixedBundleTokenAmount).toString());

        // We wait for the last of these bundles to be executable on the receiver chain.
        // By then, all of the bundles should be executable.
        await ctx.awaitInteropBundle(mixedBundleReceipt.hash);
    });

    test('Can receive a single direct call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const interopRecipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(singleDirectReceipt.hash);

        // Check the dummy interop recipient balance increased by the interop call value
        const interopRecipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((interopRecipientBalance - interopRecipientBalanceBefore).toString()).toBe(
            singleDirectAmount.toString()
        );
    });

    test('Can receive a single indirect call bundle', async () => {
        if (ctx.skipInteropTests) return;
        const recipientTokenBalanceBefore = 0n;

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(singleIndirectReceipt.hash);
        ctx.tokenA.l2AddressSecondChain = await ctx.interop2NativeTokenVault.tokenAddress(ctx.tokenA.assetId);

        // Check the token balance on the second chain was increased by the token transfer amount
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((recipientTokenBalance - recipientTokenBalanceBefore).toString()).toBe(singleIndirectAmount.toString());
    });

    test('Can receive a two direct call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const interopRecipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        const otherInteropRecipientBalanceBefore = await ctx.getInterop2Balance(ctx.otherDummyInteropRecipient);

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(twoDirectReceipt.hash);

        // Check the dummy interop recipient balance increased by the interop call value
        const interopRecipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((interopRecipientBalance - interopRecipientBalanceBefore).toString()).toBe(twoDirectAmount1.toString());
        const otherInteropRecipientBalance = await ctx.getInterop2Balance(ctx.otherDummyInteropRecipient);
        expect((otherInteropRecipientBalance - otherInteropRecipientBalanceBefore).toString()).toBe(
            twoDirectAmount2.toString()
        );
    });

    test('Can receive a two indirect call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const recipientTokenBalanceBefore = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        const otherRecipientTokenBalanceBefore = await ctx.getTokenBalance(
            ctx.otherInterop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(twoIndirectReceipt.hash);

        // Check the token balance on the second chain was increased by the token transfer amount
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((recipientTokenBalance - recipientTokenBalanceBefore).toString()).toBe(twoIndirectAmount1.toString());
        const otherRecipientTokenBalance = await ctx.getTokenBalance(
            ctx.otherInterop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((otherRecipientTokenBalance - otherRecipientTokenBalanceBefore).toString()).toBe(
            twoIndirectAmount2.toString()
        );
    });

    test('Can received a mixed call bundle', async () => {
        if (ctx.skipInteropTests) return;

        const interopRecipientBalanceBefore = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        const recipientTokenBalanceBefore = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );

        // Broadcast interop transaction from Interop1 to Interop2
        await ctx.readAndBroadcastInteropBundle(mixedBundleReceipt.hash);

        // Check the dummy interop recipient balance increased by the interop call value
        const interopRecipientBalance = await ctx.getInterop2Balance(ctx.dummyInteropRecipient);
        expect((interopRecipientBalance - interopRecipientBalanceBefore).toString()).toBe(
            mixedBundleBaseAmount.toString()
        );
        // Check the token balance on the second chain increased by the token transfer amount
        const recipientTokenBalance = await ctx.getTokenBalance(
            ctx.interop2Recipient,
            ctx.tokenA.l2AddressSecondChain!
        );
        expect((recipientTokenBalance - recipientTokenBalanceBefore).toString()).toBe(
            mixedBundleTokenAmount.toString()
        );
    });

    afterAll(async () => {
        await ctx.deinitialize();
    });
});
