import * as zksync from 'zksync-ethers-interop-support';
import * as ethers from 'ethers';

import { TestMaster } from '../src';
import { Token } from '../src/types';
import {
    scaledGasPrice,
    deployContract,
    waitUntilBlockFinalized
} from '../src/helpers';

import {
    L2_ASSET_ROUTER_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    L2_INTEROP_CENTER_ADDRESS,
    L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS,
    REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
    ETH_ADDRESS_IN_CONTRACTS,
    ArtifactInteropCenter,
    ArtifactInteropHandler,
    ArtifactNativeTokenVault,
    ArtifactMintableERC20
} from '../src/constants';
import { RetryProvider } from '../src/retry-provider';
import { getInteropTriggerData, getInteropBundleData } from '../src/temp-sdk';

const richPk = '0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110'; // Must have L1 ETH
const ethFundAmount = ethers.parseEther('1');

describe('Interop checks', () => {
    let testMaster: TestMaster;

    // L1 Variables
    let l1Provider: ethers.Provider;
    let veryRichWallet: zksync.Wallet;

    // Token details
    let tokenA: Token = {
        name: 'Token A',
        symbol: 'AA',
        decimals: 18n,
        l1Address: '',
        l2Address: '',
        l2AddressSecondChain: ''
    };

    // Interop1 (Main Chain) Variables
    let interop1Provider: zksync.Provider;
    let interop1Wallet: zksync.Wallet;
    let interop1RichWallet: zksync.Wallet;
    let interop1InteropCenter: zksync.Contract;
    let interop2InteropHandler: zksync.Contract;
    let interop1NativeTokenVault: zksync.Contract;
    let interop1TokenA: zksync.Contract;
    let aliasedInterop1WalletAddress: string;

    // Interop2 (Second Chain) Variables
    let interop2Provider: zksync.Provider;
    let interop2NativeTokenVault: zksync.Contract;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        const testWalletPK = testMaster.newEmptyAccount().privateKey;
        const mainAccount = testMaster.mainAccount();

        // Initialize providers
        l1Provider = mainAccount.providerL1!;
        interop1Provider = mainAccount.provider;
        // Setup wallets for Interop1
        veryRichWallet = new zksync.Wallet(richPk, interop1Provider, l1Provider);

        // Initialize Test Master and create wallets for Interop1
        interop1Wallet = new zksync.Wallet(testWalletPK, interop1Provider, l1Provider);
        interop1RichWallet = new zksync.Wallet(mainAccount.privateKey, interop1Provider, l1Provider);

        // Setup Interop2 Provider and Wallet
        interop2Provider = new RetryProvider(
            { url: 'http://localhost:3150', timeout: 1200 * 1000 },
            undefined,
            testMaster.reporter
        );

        // Initialize Contracts on Interop1
        interop1InteropCenter = new zksync.Contract(
            L2_INTEROP_CENTER_ADDRESS,
            ArtifactInteropCenter.abi,
            interop1Wallet
        );
        interop2InteropHandler = new zksync.Contract(
            L2_INTEROP_HANDLER_ADDRESS,
            ArtifactInteropHandler.abi,
            interop2Provider
        );
        interop1NativeTokenVault = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            interop1Wallet
        );

        // Initialize Contracts on Interop2
        interop2NativeTokenVault = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            interop2Provider
        );

        // Get the aliased address for interop calls on the first chain.
        aliasedInterop1WalletAddress = await interop2InteropHandler.getAliasedAccount(
            interop1Wallet.address,
            0 // <- Chain ID, right? Why does it work with any value right now...
        );
    });

    test('Can perform an ETH deposit', async () => {
        const gasPrice = await scaledGasPrice(interop1RichWallet);

        await (
            await veryRichWallet._signerL1!().sendTransaction({
                to: interop1RichWallet.address,
                value: ethFundAmount
            })
        ).wait();

        // Deposit funds on Interop1
        await (
            await interop1RichWallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount: ethFundAmount,
                to: interop1Wallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            })
        ).wait();
    });

    test('Can deploy token contracts', async () => {
        // Deploy Token A on Interop1
        const tokenADeploy = await deployContract(interop1Wallet, ArtifactMintableERC20, [
            tokenA.name,
            tokenA.symbol,
            tokenA.decimals
        ]);
        tokenA.l2Address = await tokenADeploy.getAddress();
        // Register Token A
        await (await interop1NativeTokenVault.registerToken(tokenA.l2Address)).wait();
        tokenA.assetId = await interop1NativeTokenVault.assetId(tokenA.l2Address);
        tokenA.l2AddressSecondChain = await interop2NativeTokenVault.tokenAddress(tokenA.assetId);
        interop1TokenA = new zksync.Contract(tokenA.l2Address, ArtifactMintableERC20.abi, interop1Wallet);
    });

    test('Can perform cross chain transfer', async () => {
        const transferAmount = 100n;

        await Promise.all([
            // Approve token transfer on Interop1
            (await interop1TokenA.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, transferAmount)).wait(),

            // Mint tokens for the test wallet on Interop1 for the transfer
            (await interop1TokenA.mint(interop1Wallet.address, transferAmount)).wait()
        ]);

        // Compose and send the interop request transaction
        const feeValue = ethers.parseEther('0.2');
        const tx = await fromInterop1RequestInterop(
            // Fee payment call starters
            [
                {
                    directCall: true,
                    nextContract: L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS,
                    data: '0x',
                    value: 0n,
                    requestedInteropCallValue: feeValue
                }
            ],
            // Execution call starters for token transfer
            [
                {
                    directCall: false,
                    nextContract: L2_ASSET_ROUTER_ADDRESS,
                    data: getTokenTransferSecondBridgeData(
                        tokenA.assetId!,
                        transferAmount,
                        aliasedInterop1WalletAddress
                    ),
                    value: 0n,
                    requestedInteropCallValue: 0n
                }
            ]
        );

        // Broadcast interop transaction from Interop1 to Interop2
        await readAndBroadcastInteropTx(tx.hash, interop1Provider, interop2Provider);

        tokenA.l2AddressSecondChain = await interop2NativeTokenVault.tokenAddress(tokenA.assetId);
        console.log('Token A info:', tokenA);

        // Assert that the token balance on chain2
        const interop1WalletSecondChainBalance = await getTokenBalance({
            provider: interop2Provider,
            tokenAddress: tokenA.l2AddressSecondChain!,
            address: aliasedInterop1WalletAddress
        });
        expect(interop1WalletSecondChainBalance).toBe(transferAmount);
    });

    // Types for interop call starters and gas fields.
    interface InteropCallStarter {
        directCall: boolean;
        nextContract: string;
        data: string;
        value: bigint;
        // The interop call value must be pre-determined.
        requestedInteropCallValue: bigint;
    }

    /**
     * Sends a direct L2 transaction request on Interop1.
     * The function prepares the interop call input and populates the transaction before sending.
     */
    async function fromInterop1RequestInterop(
        feeCallStarters: InteropCallStarter[],
        execCallStarters: InteropCallStarter[]
    ) {
        const destinationChainId = (await interop2Provider.getNetwork()).chainId.toString(16);

        const tx = await interop1InteropCenter.requestInterop(
            destinationChainId,
            L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS,
            feeCallStarters,
            execCallStarters,
            {
                gasLimit: 30000000n,
                gasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
                refundRecipient: interop1Wallet.address,
                paymaster: ethers.ZeroAddress,
                paymasterInput: '0x'
            },
            {
                value: [...feeCallStarters, ...execCallStarters].reduce(
                    (total, item) => total + BigInt(item.requestedInteropCallValue),
                    0n
                )
            }
        );
        const txReceipt: zksync.types.TransactionReceipt = await tx.wait();
        return txReceipt;
    }

    /**
     * Generates ABI-encoded data for transferring tokens using the second bridge.
     */
    function getTokenTransferSecondBridgeData(assetId: string, amount: bigint, recipient: string) {
        return ethers.concat([
            '0x01',
            new ethers.AbiCoder().encode(
                ['bytes32', 'bytes'],
                [
                    assetId,
                    new ethers.AbiCoder().encode(
                        ['uint256', 'address', 'address'],
                        [amount, recipient, ethers.ZeroAddress]
                    )
                ]
            )
        ]);
    }

    /**
     * Reads an interop transaction from the sender chain, constructs a new transaction,
     * and broadcasts it on the receiver chain.
     */
    async function readAndBroadcastInteropTx(
        txHash: string,
        senderProvider: zksync.Provider,
        receiverProvider: zksync.Provider
    ) {
        console.log('*Reading and broadcasting interop tx initiated by txHash*', txHash);
        const senderUtilityWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, senderProvider);
        const txReceipt = await senderProvider.getTransactionReceipt(txHash);
        await waitUntilBlockFinalized(senderUtilityWallet, txReceipt!.blockNumber);

        // Get interop trigger and bundle data from the sender chain.
        const triggerDataBundle = await getInteropTriggerData(senderProvider, txHash, 2);
        const feeBundle = await getInteropBundleData(senderProvider, txHash, 0);
        const executionBundle = await getInteropBundleData(senderProvider, txHash, 1);
        if (triggerDataBundle.output == null) return;

        // ABI-encode execution data along with its proof.
        const txData = ethers.AbiCoder.defaultAbiCoder().encode(
            ['bytes', 'bytes'],
            [executionBundle.rawData, executionBundle.fullProof]
        );

        // Construct the interop transaction for the receiver chain.
        const nonce = await receiverProvider.getTransactionCount(L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS);
        const feeData = await receiverProvider.getFeeData();
        let interopTx = {
            from: L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS,
            to: L2_INTEROP_HANDLER_ADDRESS,
            chainId: (await receiverProvider.getNetwork()).chainId.toString(),
            data: txData,
            nonce: nonce,
            customData: {
                paymasterParams: {
                    paymaster: triggerDataBundle.output.gasFields.paymaster,
                    paymasterInput: triggerDataBundle.output.gasFields.paymasterInput
                },
                gasPerPubdata: triggerDataBundle.output.gasFields.gasPerPubdataByteLimit,
                customSignature: ethers.AbiCoder.defaultAbiCoder().encode(
                    ['bytes', 'bytes', 'address', 'address', 'bytes'],
                    [
                        feeBundle.rawData,
                        feeBundle.fullProof,
                        triggerDataBundle.output.from,
                        triggerDataBundle.output.gasFields.refundRecipient,
                        triggerDataBundle.fullProof
                    ]
                )
            },
            maxFeePerGas: feeData.maxFeePerGas,
            maxPriorityFeePerGas: feeData.maxPriorityFeePerGas,
            gasLimit: triggerDataBundle.output.gasFields.gasLimit,
            value: 0
        };

        // Serialize and broadcast the transaction
        const hexTx = zksync.utils.serializeEip712(interopTx);
        const broadcastTx = await receiverProvider.broadcastTransaction(hexTx);
        await broadcastTx.wait();

        // Recursive broadcast
        await readAndBroadcastInteropTx(broadcastTx.realInteropHash!, receiverProvider, senderProvider);
    }

    /**
     * Retrieves the token balance for a given address.
     */
    async function getTokenBalance({
        provider,
        tokenAddress,
        address
    }: {
        provider: zksync.Provider;
        tokenAddress: string;
        address: string;
    }): Promise<bigint> {
        if (!tokenAddress) {
            throw new Error('Token address is not provided');
        }
        if (tokenAddress === ethers.ZeroAddress) {
            // Happens when token wasn't deployed yet. Therefore there is no balance.
            return 0n;
        }
        const tokenContract = new zksync.Contract(tokenAddress, ArtifactMintableERC20.abi, provider);
        return await tokenContract.balanceOf(address);
    }
});
