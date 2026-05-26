/**
 * This suite contains tests checking Interop-B bundle/message features,
 * those that were included as part of v31 protocol upgrade.
 */

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { TransactionReceipt } from 'ethers';
import { formatEvmV1Address, formatEvmV1Chain, getInteropBundleData } from 'highlevel-test-tools/src/temp-sdk';

import { TestMaster } from '../src';
import { deployContract, getGWBlockNumber, getL2bUrl, waitForInteropRootNonZero } from '../src/helpers';
import { waitUntilBlockExecutedOnGateway, waitUntilBlockFinalized } from '../src/helpers';
import {
    ArtifactDummyInteropRecipient,
    ArtifactIERC7786Attributes,
    ArtifactInteropCenter,
    ArtifactInteropHandler,
    ArtifactL1BridgeHub,
    ArtifactNativeTokenVault,
    L2_ASSET_ROUTER_ADDRESS,
    L2_INTEROP_CENTER_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS
} from '../src/constants';
import { RetryProvider, RetryableWallet } from '../src/retry-provider';
import { Token } from '../src/types';

type SentInteropData = {
    bundle: { amount: bigint; receipt: TransactionReceipt };
    message: { amount: bigint; receipt: TransactionReceipt };
};

describe('Interop-B behavior checks', () => {
    let testMaster: TestMaster;
    let ctx: ReadyInteropBContext | null = null;
    const sentInteropData: Partial<SentInteropData> = {};

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        ctx = await createInteropBContext(testMaster);
    });

    test('Can send one interop bundle and one interop message', async () => {
        if (!ctx) return;

        const bundle = await sendSimpleBundle(ctx);
        const message = await sendSimpleMessage(ctx);
        sentInteropData.bundle = bundle;
        sentInteropData.message = message;

        // Waiting on the latest tx hash is enough: earlier interop txs are executable by then.
        await awaitInteropBundle(ctx, message.receipt.hash);
    });

    test('Can receive an interop bundle', async () => {
        if (!ctx) return;

        const sent = getSentInteropData(sentInteropData);
        const bridgedTokenAssetId = requireString(ctx.bridgedToken.assetId, 'bridgedToken.assetId');
        const tokenAddressBefore = await ctx.interop2NativeTokenVault.tokenAddress(bridgedTokenAssetId);
        const bundleRecipientTokenBalanceBefore = await getTokenBalance(
            ctx.bundleRecipientAddress,
            tokenAddressBefore,
            ctx.interop2Provider
        );

        await readAndBroadcastInteropBundle(ctx, sent.bundle.receipt.hash);

        const tokenAddressAfter = await ctx.interop2NativeTokenVault.tokenAddress(bridgedTokenAssetId);
        const bundleRecipientTokenBalanceAfter = await getTokenBalance(
            ctx.bundleRecipientAddress,
            tokenAddressAfter,
            ctx.interop2Provider
        );

        expect((bundleRecipientTokenBalanceAfter - bundleRecipientTokenBalanceBefore).toString()).toBe(
            sent.bundle.amount.toString()
        );
    });

    test('Can receive an interop message', async () => {
        if (!ctx) return;

        const sent = getSentInteropData(sentInteropData);
        const bridgedTokenAssetId = requireString(ctx.bridgedToken.assetId, 'bridgedToken.assetId');
        const tokenAddressBefore = await ctx.interop2NativeTokenVault.tokenAddress(bridgedTokenAssetId);
        const messageRecipientTokenBalanceBefore = await getTokenBalance(
            ctx.messageRecipientAddress,
            tokenAddressBefore,
            ctx.interop2Provider
        );

        await readAndBroadcastInteropBundle(ctx, sent.message.receipt.hash);

        const tokenAddressAfter = await ctx.interop2NativeTokenVault.tokenAddress(bridgedTokenAssetId);
        const messageRecipientTokenBalanceAfter = await getTokenBalance(
            ctx.messageRecipientAddress,
            tokenAddressAfter,
            ctx.interop2Provider
        );

        expect((messageRecipientTokenBalanceAfter - messageRecipientTokenBalanceBefore).toString()).toBe(
            sent.message.amount.toString()
        );
    });

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});

interface InteropCallStarter {
    to: string;
    data: string;
    callAttributes: string[];
}

type BalanceSnapshot = {
    native: bigint;
    token: bigint;
    tokenAddress: string;
};

type ReadyInteropBContext = {
    interop1Provider: zksync.Provider;
    interop2Provider: zksync.Provider;
    interop2ChainId: bigint;
    interop1Wallet: RetryableWallet;
    interop2RichWallet: RetryableWallet;
    bundleRecipientAddress: string;
    messageRecipientAddress: string;
    gatewayWallet: zksync.Wallet;
    interop1InteropCenter: zksync.Contract;
    interop2InteropHandler: zksync.Contract;
    interop1NativeTokenVault: zksync.Contract;
    interop2NativeTokenVault: zksync.Contract;
    bridgedToken: Token;
    erc7786AttributesInterface: ethers.Interface;
};

function getSentInteropData(sentInteropData: Partial<SentInteropData>): SentInteropData {
    if (!sentInteropData.bundle || !sentInteropData.message) {
        throw new Error('Interop payloads were not sent yet');
    }
    return sentInteropData as SentInteropData;
}

async function createInteropBContext(testMaster: TestMaster): Promise<ReadyInteropBContext | null> {
    const mainAccount = testMaster.mainAccount();
    const bridgehub = new ethers.Contract(
        await mainAccount.provider.getBridgehubContractAddress(),
        ArtifactL1BridgeHub.abi,
        mainAccount.providerL1
    );
    const interop1ChainId = (await mainAccount.provider.getNetwork()).chainId;
    const l1ChainId = (await mainAccount.providerL1!.getNetwork()).chainId;

    // Skip Interop-B tests when settlement layer is the same as L1.
    if ((await bridgehub.settlementLayer(interop1ChainId)) == l1ChainId) {
        return null;
    }

    const mainAccountSecondChain = testMaster.mainAccountSecondChain();
    if (!mainAccountSecondChain) {
        throw new Error(
            'Interop tests cannot be run if the second chain is not set. Use the --second-chain flag to specify a different second chain to run the tests on.'
        );
    }

    const interop2Provider = mainAccountSecondChain.provider;
    const interop2ChainId = (await interop2Provider.getNetwork()).chainId;

    const gatewayProvider = new RetryProvider(
        { url: await getL2bUrl('gateway'), timeout: 1200 * 1000 },
        undefined,
        testMaster.reporter
    );
    const gatewayWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, gatewayProvider);

    const interop1InteropCenter = new zksync.Contract(
        L2_INTEROP_CENTER_ADDRESS,
        ArtifactInteropCenter.abi,
        mainAccount
    );
    const interop2InteropHandler = new zksync.Contract(
        L2_INTEROP_HANDLER_ADDRESS,
        ArtifactInteropHandler.abi,
        mainAccountSecondChain
    );
    const interop1NativeTokenVault = new zksync.Contract(
        L2_NATIVE_TOKEN_VAULT_ADDRESS,
        ArtifactNativeTokenVault.abi,
        mainAccount
    );
    const interop2NativeTokenVault = new zksync.Contract(
        L2_NATIVE_TOKEN_VAULT_ADDRESS,
        ArtifactNativeTokenVault.abi,
        interop2Provider
    );

    const bridgedToken: Token = { ...testMaster.environment().erc20Token };
    bridgedToken.assetId = await interop1NativeTokenVault.assetId(bridgedToken.l2Address);

    const bundleRecipientContract = await deployContract(mainAccountSecondChain, ArtifactDummyInteropRecipient, []);
    const messageRecipientContract = await deployContract(mainAccountSecondChain, ArtifactDummyInteropRecipient, []);
    const bundleRecipientAddress = await bundleRecipientContract.getAddress();
    const messageRecipientAddress = await messageRecipientContract.getAddress();

    return {
        interop1Provider: mainAccount.provider,
        interop2Provider,
        interop2ChainId,
        interop1Wallet: mainAccount,
        interop2RichWallet: mainAccountSecondChain,
        bundleRecipientAddress,
        messageRecipientAddress,
        gatewayWallet,
        interop1InteropCenter,
        interop2InteropHandler,
        interop1NativeTokenVault,
        interop2NativeTokenVault,
        bridgedToken,
        erc7786AttributesInterface: new ethers.Interface(ArtifactIERC7786Attributes.abi)
    };
}

async function sendSimpleBundle(ctx: ReadyInteropBContext): Promise<SentInteropData['bundle']> {
    const amount = 111n;
    await approveInteropTokenTransfer(ctx, amount);
    const before = await captureInterop1BalanceSnapshot(ctx, ctx.bridgedToken.l2Address);
    const bridgedTokenAssetId = requireString(ctx.bridgedToken.assetId, 'bridgedToken.assetId');

    const execCallStarters: InteropCallStarter[] = [
        {
            to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
            data: getTokenTransferSecondBridgeData(bridgedTokenAssetId, amount, ctx.bundleRecipientAddress),
            callAttributes: [ctx.erc7786AttributesInterface.encodeFunctionData('indirectCall', [0n])]
        }
    ];

    const msgValue = await calculateMsgValue(ctx, execCallStarters.length);
    const bundleAttributes = [ctx.erc7786AttributesInterface.encodeFunctionData('useFixedFee', [false])];
    const tx = await ctx.interop1InteropCenter.sendBundle(
        formatEvmV1Chain(ctx.interop2ChainId),
        execCallStarters,
        bundleAttributes,
        { value: msgValue }
    );
    const receipt = await waitForReceipt(tx.wait(), 'interop bundle');

    await assertInterop1BalanceChanges(ctx, receipt, before, {
        msgValue,
        tokenAmount: amount
    });
    return { amount, receipt };
}

async function sendSimpleMessage(ctx: ReadyInteropBContext): Promise<SentInteropData['message']> {
    const amount = 222n;
    await approveInteropTokenTransfer(ctx, amount);
    const before = await captureInterop1BalanceSnapshot(ctx, ctx.bridgedToken.l2Address);
    const bridgedTokenAssetId = requireString(ctx.bridgedToken.assetId, 'bridgedToken.assetId');
    const assetRouterRecipient = formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS, ctx.interop2ChainId);

    const msgValue = await calculateMsgValue(ctx, 1);
    const tx = await ctx.interop1InteropCenter.sendMessage(
        assetRouterRecipient,
        getTokenTransferSecondBridgeData(bridgedTokenAssetId, amount, ctx.messageRecipientAddress),
        [
            ctx.erc7786AttributesInterface.encodeFunctionData('indirectCall', [0n]),
            ctx.erc7786AttributesInterface.encodeFunctionData('executionAddress', [
                formatEvmV1Address(ctx.interop2RichWallet.address, ctx.interop2ChainId)
            ]),
            ctx.erc7786AttributesInterface.encodeFunctionData('useFixedFee', [false])
        ],
        { value: msgValue }
    );
    const receipt = await waitForReceipt(tx.wait(), 'interop message');

    await assertInterop1BalanceChanges(ctx, receipt, before, {
        msgValue,
        tokenAmount: amount
    });
    return { amount, receipt };
}

async function awaitInteropBundle(ctx: ReadyInteropBContext, txHash: string) {
    const senderUtilityWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, ctx.interop1Provider);
    const txReceipt = await ctx.interop1Provider.getTransactionReceipt(txHash);
    if (!txReceipt) {
        throw new Error(`No receipt found for ${txHash}`);
    }

    await waitUntilBlockFinalized(senderUtilityWallet, txReceipt.blockNumber);
    await waitUntilBlockExecutedOnGateway(senderUtilityWallet, ctx.gatewayWallet, txReceipt.blockNumber);

    // Extra delay to reduce flakiness around root propagation.
    await zksync.utils.sleep(1000);
    const params = await senderUtilityWallet.getFinalizeWithdrawalParams(txHash, 0, 'proof_based_gw');
    await waitForInteropRootNonZero(ctx.interop2Provider, ctx.interop2RichWallet, getGWBlockNumber(params));
}

async function readAndBroadcastInteropBundle(ctx: ReadyInteropBContext, txHash: string) {
    const executionBundle = await getInteropBundleData(ctx.interop1Provider, txHash, 0);
    if (executionBundle.output == null) {
        throw new Error(`No interop output found for ${txHash}`);
    }

    const executeTx = await ctx.interop2InteropHandler.executeBundle(
        executionBundle.rawData,
        executionBundle.proofDecoded
    );
    await waitForReceipt(executeTx.wait(), 'interop execution');
}

async function captureInterop1BalanceSnapshot(
    ctx: ReadyInteropBContext,
    tokenAddress: string
): Promise<BalanceSnapshot> {
    return {
        native: await ctx.interop1Wallet.getBalance(),
        token: await getTokenBalance(ctx.interop1Wallet.address, tokenAddress, ctx.interop1Provider),
        tokenAddress
    };
}

async function assertInterop1BalanceChanges(
    ctx: ReadyInteropBContext,
    receipt: TransactionReceipt,
    before: BalanceSnapshot,
    expected: { msgValue: bigint; tokenAmount: bigint }
) {
    if (receipt.gasPrice == null) {
        throw new Error('Missing gas price in receipt');
    }
    const feePaid = receipt.gasUsed * receipt.gasPrice;
    const afterNative = await ctx.interop1Wallet.getBalance();
    expect(afterNative.toString()).toBe((before.native - feePaid - expected.msgValue).toString());

    const afterToken = await getTokenBalance(ctx.interop1Wallet.address, before.tokenAddress, ctx.interop1Provider);
    expect(afterToken.toString()).toBe((before.token - expected.tokenAmount).toString());
}

async function approveInteropTokenTransfer(ctx: ReadyInteropBContext, amount: bigint) {
    const bridgedTokenContract = new zksync.Contract(
        ctx.bridgedToken.l2Address,
        zksync.utils.IERC20,
        ctx.interop1Wallet
    );
    const approveTx = await bridgedTokenContract.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, amount);
    await waitForReceipt(approveTx.wait(), 'token approval');
}

async function calculateMsgValue(ctx: ReadyInteropBContext, numCalls: number): Promise<bigint> {
    const interopFeesTotal = (await ctx.interop1InteropCenter.interopProtocolFee()) * BigInt(numCalls);
    return interopFeesTotal;
}

function getTokenTransferSecondBridgeData(assetId: string, amount: bigint, recipient: string) {
    return ethers.concat([
        '0x01',
        new ethers.AbiCoder().encode(
            ['bytes32', 'bytes'],
            [
                assetId,
                new ethers.AbiCoder().encode(['uint256', 'address', 'address'], [amount, recipient, ethers.ZeroAddress])
            ]
        )
    ]);
}

async function getTokenBalance(address: string, tokenAddress: string, provider: zksync.Provider): Promise<bigint> {
    if (tokenAddress === ethers.ZeroAddress) return 0n;
    const tokenContract = new zksync.Contract(tokenAddress, zksync.utils.IERC20, provider);
    const balance = await tokenContract.balanceOf(address);
    return balance;
}

async function waitForReceipt(
    receiptPromise: Promise<TransactionReceipt | null>,
    label: string
): Promise<TransactionReceipt> {
    const receipt = await receiptPromise;
    if (!receipt) {
        throw new Error(`No receipt for ${label}`);
    }
    return receipt;
}

function requireString(value: string | undefined, label: string): string {
    if (!value) {
        throw new Error(`${label} is not initialized`);
    }
    return value;
}
