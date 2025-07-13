/**
 * This suite contains tests checking interop functionality.
 */

import { TestMaster } from '../src';
import { Token } from '../src/types';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';

import { waitForL2ToL1LogProof } from '../src/helpers';
import { RetryableWallet } from '../src/retry-provider';

import { scaledGasPrice, deployContract, waitUntilBlockFinalized, getL2bUrl } from '../src/helpers';

import {
    L2_ASSET_ROUTER_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    L2_INTEROP_CENTER_ADDRESS,
    L2_INTEROP_ROOT_STORAGE_ADDRESS,
    L2_MESSAGE_VERIFICATION_ADDRESS,
    REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
    ETH_ADDRESS_IN_CONTRACTS,
    ArtifactBridgeHub,
    ArtifactInteropCenter,
    ArtifactInteropHandler,
    ArtifactNativeTokenVault,
    ArtifactMintableERC20,
    ArtifactIERC7786Attributes,
    ArtifactL2InteropRootStorage,
    ArtifactL2MessageVerification,
    GATEWAY_CHAIN_ID
} from '../src/constants';
import { RetryProvider } from '../src/retry-provider';
import { getInteropBundleData } from '../src/temp-sdk';
import { ETH_ADDRESS, sleep } from 'zksync-ethers/build/utils';
import { FinalizeWithdrawalParams } from 'zksync-ethers/build/types';

const richPk = '0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110'; // Must have L1 ETH
const ethFundAmount = ethers.parseEther('1');

describe('Interop behavior checks', () => {
    let testMaster: TestMaster;
    let alice: RetryableWallet;
    let aliceSecondChain: RetryableWallet;
    let tokenDetails: Token;

    let skipInteropTests = false;

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
    // kl todo remove very rich wallet. Useful for local debugging, calldata can be sent directly using cast.
    let interop1VeryRichWallet: zksync.Wallet;
    let interop1InteropCenter: zksync.Contract;
    let interop2InteropHandler: zksync.Contract;
    let interop1NativeTokenVault: zksync.Contract;
    let interop1TokenA: zksync.Contract;

    // Interop2 (Second Chain) Variables
    let interop2RichWallet: zksync.Wallet;
    let interop2Provider: zksync.Provider;
    let interop2NativeTokenVault: zksync.Contract;

    // Gateway Variables
    let gatewayProvider: zksync.Provider;
    let gatewayWallet: zksync.Wallet;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        alice = testMaster.mainAccount();

        tokenDetails = testMaster.environment().erc20Token;

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
        interop1VeryRichWallet = new zksync.Wallet(richPk, interop1Provider, l1Provider);

        // Skip interop tests if the SL is the same as the L1.
        const bridgehub = new ethers.Contract(
            await alice.provider.getBridgehubContractAddress(),
            ArtifactBridgeHub.abi,
            alice.providerL1
        );

        if (
            (await bridgehub.settlementLayer((await alice.provider.getNetwork()).chainId)) ==
            (await alice.providerL1!.getNetwork()).chainId
        ) {
            skipInteropTests = true;
        } else {
            // Define the second chain wallet if the SL is different from the L1.
            const maybeAliceSecondChain = testMaster.mainAccountSecondChain();
            if (!maybeAliceSecondChain) {
                throw new Error(
                    'Interop tests cannot be run if the second chain is not set. Use the --second-chain flag to specify a different second chain to run the tests on.'
                );
            }
            aliceSecondChain = maybeAliceSecondChain!;
        }

        // Setup Interop2 Provider and Wallet
        interop2Provider = new RetryProvider(
            { url: await getL2bUrl('validium'), timeout: 1200 * 1000 },
            undefined,
            testMaster.reporter
        );
        interop2RichWallet = new zksync.Wallet(mainAccount.privateKey, interop2Provider, l1Provider);

        gatewayProvider = new RetryProvider(
            { url: await getL2bUrl('gateway'), timeout: 1200 * 1000 },
            undefined,
            testMaster.reporter
        );
        gatewayWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, gatewayProvider);

        // Initialize Contracts on Interop1
        interop1InteropCenter = new zksync.Contract(
            L2_INTEROP_CENTER_ADDRESS,
            ArtifactInteropCenter.abi,
            interop1Wallet
        );
        interop2InteropHandler = new zksync.Contract(
            L2_INTEROP_HANDLER_ADDRESS,
            ArtifactInteropHandler.abi,
            interop2RichWallet
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
    });

    let withdrawalHash: string;
    let params: FinalizeWithdrawalParams;
    test('Can check withdrawal hash in L2-A', async () => {
        if (skipInteropTests) {
            return;
        }

        const l2MessageVerification = new zksync.Contract(
            L2_MESSAGE_VERIFICATION_ADDRESS,
            ArtifactL2MessageVerification.abi,
            alice.provider
        );

        // Perform a withdrawal and wait for it to be processed
        const withdrawalPromise = await alice.withdraw({
            token: tokenDetails.l2Address,
            amount: 1n
        });
        await expect(withdrawalPromise).toBeAccepted();
        const withdrawalTx = await withdrawalPromise;
        withdrawalHash = withdrawalTx.hash;
        const l2TxReceipt = await alice.provider.getTransactionReceipt(withdrawalHash);
        await waitForL2ToL1LogProof(alice, l2TxReceipt!.blockNumber, withdrawalHash);

        // Proof-based interop on Gateway, meaning the Merkle proof hashes to Gateway's MessageRoot
        params = await alice.getFinalizeWithdrawalParams(withdrawalHash, undefined, 'proof_based_gw');

        // Needed else the L2's view of GW's MessageRoot won't be updated
        await waitForInteropRootNonZero(alice.provider, alice, GW_CHAIN_ID, getGWBlockNumber(params));

        const included = await l2MessageVerification.proveL2MessageInclusionShared(
            (await alice.provider.getNetwork()).chainId,
            params.l1BatchNumber,
            params.l2MessageIndex,
            { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
            params.proof
        );
        expect(included).toBe(true);
    });

    test('Can check withdrawal hash from L2-B', async () => {
        if (skipInteropTests) {
            return;
        }

        const l2MessageVerification = new zksync.Contract(
            L2_MESSAGE_VERIFICATION_ADDRESS,
            ArtifactL2MessageVerification.abi,
            aliceSecondChain.provider
        );

        // Needed else the L2's view of GW's MessageRoot won't be updated
        await waitForInteropRootNonZero(
            aliceSecondChain.provider,
            aliceSecondChain,
            GW_CHAIN_ID,
            getGWBlockNumber(params)
        );

        // We use the same proof that was verified in L2-A
        const included = await l2MessageVerification.proveL2MessageInclusionShared(
            (await alice.provider.getNetwork()).chainId,
            params.l1BatchNumber,
            params.l2MessageIndex,
            { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
            params.proof
        );
        expect(included).toBe(true);
    });

    test('Can perform an ETH deposit', async () => {
        const gasPrice = await scaledGasPrice(interop1RichWallet);

        await (
            await veryRichWallet._signerL1!().sendTransaction({
                to: interop1RichWallet.address,
                value: ethFundAmount * 10n
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

        // Deposit funds on Interop2
        await (
            await interop2RichWallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount: ethFundAmount,
                to: interop2RichWallet.address,
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
        console.log('tokenA.l2Address', tokenA.l2Address);
        // Register Token A
        await (await interop1NativeTokenVault.registerToken(tokenA.l2Address)).wait();
        tokenA.assetId = await interop1NativeTokenVault.assetId(tokenA.l2Address);
        tokenA.l2AddressSecondChain = await interop2NativeTokenVault.tokenAddress(tokenA.assetId);
        interop1TokenA = new zksync.Contract(tokenA.l2Address, ArtifactMintableERC20.abi, interop1Wallet);
    });

    test('Can perform cross chain transfer', async () => {
        const transferAmount = 100n;
        // let interop1TokenAVeryRichWallet = new zksync.Contract(
        //     tokenA.l2Address,
        //     ArtifactMintableERC20.abi,
        //     interop1VeryRichWallet
        // );
        // await ((await interop1TokenAVeryRichWallet.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, transferAmount)).wait()).wait();
        // await ((await interop1TokenA.mint('0x36615Cf349d7F6344891B1e7CA7C72883F5dc049', transferAmount)).wait()).wait();

        await Promise.all([
            // Approve token transfer on Interop1
            (await interop1TokenA.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, transferAmount)).wait(),

            // Mint tokens for the test wallet on Interop1 for the transfer
            (await interop1TokenA.mint(interop1Wallet.address, transferAmount)).wait()
        ]);

        // Compose and send the interop request transaction
        const erc7786AttributeDummy = new zksync.Contract(
            '0x0000000000000000000000000000000000000000',
            ArtifactIERC7786Attributes.abi,
            interop1Wallet
        );

        const feeValue = ethers.parseEther('0.2');
        const receipt = await fromInterop1RequestInterop(
            // Fee payment call starters
            [
                {
                    nextContract: ethers.ZeroAddress,
                    data: '0x',
                    callAttributes: [
                        await erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [feeValue])
                    ]
                }
            ],
            // Execution call starters for token transfer
            [
                {
                    nextContract: L2_ASSET_ROUTER_ADDRESS,
                    data: getTokenTransferSecondBridgeData(tokenA.assetId!, transferAmount, interop2RichWallet.address),
                    callAttributes: [await erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0n])]
                }
            ]
        );
        // console.log('receipt', receipt);

        // Broadcast interop transaction from Interop1 to Interop2
        // await readAndBroadcastInteropTx(tx.hash, interop1Provider, interop2Provider);
        await readAndBroadcastInteropBundle(receipt.hash, interop1Provider, interop2Provider);

        await sleep(10000);
        tokenA.l2AddressSecondChain = await interop2NativeTokenVault.tokenAddress(tokenA.assetId);
        console.log('Token A info:', tokenA);

        // Assert that the token balance on chain2
        // const interop1WalletSecondChainBalance = await getTokenBalance({
        //     provider: interop2Provider,
        //     tokenAddress: tokenA.l2AddressSecondChain!,
        //     address: interop2RichWallet.address
        // });
        // expect(interop1WalletSecondChainBalance).toBe(transferAmount);
    });

    // Types for interop call starters and gas fields.
    interface InteropCallStarter {
        nextContract: string;
        data: string;
        callAttributes: string[];
    }

    /**
     * Sends a direct L2 transaction request on Interop1.
     * The function prepares the interop call input and populates the transaction before sending.
     */
    async function fromInterop1RequestInterop(
        feeCallStarters: InteropCallStarter[],
        execCallStarters: InteropCallStarter[]
    ) {
        // note skipping feeCallStarters for now:

        const txFinalizeReceipt = (
            await interop1InteropCenter.sendBundle((await interop2Provider.getNetwork()).chainId, execCallStarters, [])
        ).wait();
        return txFinalizeReceipt;

        // const tx = await interop1InteropCenter.requestInterop(
        //     (await interop2Provider.getNetwork()).chainId,
        //     // L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS,
        //     feeCallStarters,
        //     execCallStarters,
        //     {
        //         gasLimit: 30000000n,
        //         gasPerPubdataByteLimit: REQUIRED_L2_GAS_PRICE_PER_PUBDATA,
        //         refundRecipient: interop1Wallet.address,
        //         paymaster: ethers.ZeroAddress,
        //         paymasterInput: '0x'
        //     },
        //     {
        //         value: [...feeCallStarters, ...execCallStarters].reduce(
        //             (total, item) => total + BigInt(item.requestedInteropCallValue),
        //             0n
        //         )
        //     }
        // );
        // const txReceipt: zksync.types.TransactionReceipt = await tx.wait();
        // return txReceipt;
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

    function getGWBlockNumber(params: FinalizeWithdrawalParams): number {
        /// see hashProof in MessageHashing.sol for this logic.
        let gwProofIndex =
            1 + parseInt(params.proof[0].slice(4, 6), 16) + 1 + parseInt(params.proof[0].slice(6, 8), 16);
        // console.log('params', params, gwProofIndex, parseInt(params.proof[gwProofIndex].slice(2, 34), 16));
        return parseInt(params.proof[gwProofIndex].slice(2, 34), 16);
    }

    async function waitForInteropRootNonZero(
        provider: zksync.Provider,
        alice: zksync.Wallet,
        chainId: bigint,
        l1BatchNumber: number
    ) {
        const l2InteropRootStorage = new zksync.Contract(
            L2_INTEROP_ROOT_STORAGE_ADDRESS,
            ArtifactL2InteropRootStorage.abi,
            provider
        );
        let currentRoot = ethers.ZeroHash;
        let count = 0;
        while (currentRoot === ethers.ZeroHash && count < 20) {
            // We make repeated transactions to force the L2 to update the interop root.
            const tx = await alice.transfer({
                to: alice.address,
                amount: 1,
                token: ETH_ADDRESS
            });
            await tx.wait();

            currentRoot = await l2InteropRootStorage.interopRoots(parseInt(chainId.toString()), l1BatchNumber);
            await zksync.utils.sleep(alice.provider.pollingInterval);

            console.log('currentRoot', currentRoot, count);
            count++;
        }
        console.log('Interop root is non-zero', currentRoot, l1BatchNumber);
    }

    const GW_CHAIN_ID = 506n;

    /**
     * Reads an interop transaction from the sender chain, constructs a new transaction,
     * and broadcasts it on the receiver chain.
     */
    async function readAndBroadcastInteropBundle(
        txHash: string,
        senderProvider: zksync.Provider,
        receiverProvider: zksync.Provider
    ) {
        console.log('*Reading and broadcasting interop bundle initiated by txHash*', txHash);
        const senderUtilityWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, senderProvider);
        const txReceipt = await senderProvider.getTransactionReceipt(txHash);
        await waitUntilBlockFinalized(senderUtilityWallet, txReceipt!.blockNumber);
        // const gwWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, gatewayProvider);
        // await waitUntilBlockExecutedOnGateway(senderUtilityWallet, gatewayWallet, txReceipt!.blockNumber);
        /// kl todo figure out what we need to wait for here. Probably the fact that we need to wait for the GW block finalization.
        await sleep(25000);
        // console.log((await senderProvider.getNetwork()).chainId);
        // console.log((await senderProvider.getNetwork()).name)
        // console.log(await senderUtilityWallet.getL2BridgeContracts())
        const params = await senderUtilityWallet.getFinalizeWithdrawalParams(txHash, 0, 'proof_based_gw');
        await waitForInteropRootNonZero(interop2Provider, interop2RichWallet, GW_CHAIN_ID, getGWBlockNumber(params));

        // Get interop trigger and bundle data from the sender chain.
        const executionBundle = await getInteropBundleData(senderProvider, txHash, 0);
        // console.log('executionBundle', executionBundle);
        if (executionBundle.output == null) return;

        const receipt = await interop2InteropHandler.executeBundle(
            executionBundle.rawData,
            executionBundle.proofDecoded
        );
        await receipt.wait();
        console.log('receipt', receipt.hash);
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

        /// kl todo figure out what we need to wait for here. Probably the fact that we need to wait for the GW block finalization.
        await sleep(25000);
        const params = await senderUtilityWallet.getFinalizeWithdrawalParams(txHash, 0, 'proof_based_gw');
        await waitForInteropRootNonZero(interop2Provider, interop2RichWallet, GW_CHAIN_ID, getGWBlockNumber(params));

        // Get interop trigger and bundle data from the sender chain.
        // const triggerDataBundle = await getInteropTriggerData(senderProvider, txHash, 2);
        // const feeBundle = await getInteropBundleData(senderProvider, txHash, 0);
        const executionBundle = await getInteropBundleData(senderProvider, txHash, 0);
        if (executionBundle.output == null) return;

        // ABI-encode execution data along with its proof.
        const txData = ethers.AbiCoder.defaultAbiCoder().encode(
            ['bytes', 'bytes'],
            [executionBundle.rawData, executionBundle.fullProof]
        );

        // Construct the interop transaction for the receiver chain.
        // const nonce = await receiverProvider.getTransactionCount(L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS);
        const feeData = await receiverProvider.getFeeData();
        // let interopTx = {
        //     from: L2_STANDARD_TRIGGER_ACCOUNT_ADDRESS,
        //     to: L2_INTEROP_HANDLER_ADDRESS,
        //     chainId: (await receiverProvider.getNetwork()).chainId.toString(),
        //     data: txData,
        //     nonce: nonce,
        //     customData: {
        //         paymasterParams: {
        //             paymaster: triggerDataBundle.output.gasFields.paymaster,
        //             paymasterInput: triggerDataBundle.output.gasFields.paymasterInput
        //         },
        //         gasPerPubdata: triggerDataBundle.output.gasFields.gasPerPubdataByteLimit,
        //         customSignature: ethers.AbiCoder.defaultAbiCoder().encode(
        //             ['bytes', 'bytes', 'address', 'address', 'bytes'],
        //             [
        //                 feeBundle.rawData,
        //                 feeBundle.fullProof,
        //                 triggerDataBundle.output.sender,
        //                 triggerDataBundle.output.gasFields.refundRecipient,
        //                 triggerDataBundle.fullProof
        //             ]
        //         )
        //     },
        //     maxFeePerGas: feeData.maxFeePerGas,
        //     maxPriorityFeePerGas: feeData.maxPriorityFeePerGas,
        //     gasLimit: triggerDataBundle.output.gasFields.gasLimit,
        //     value: 0
        // };

        // Serialize and broadcast the transaction
        // const hexTx = zksync.utils.serializeEip712(interopTx);
        // const broadcastTx = await receiverProvider.broadcastTransaction(hexTx);
        // await broadcastTx.wait();

        // Recursive broadcast
        // await readAndBroadcastInteropTx(broadcastTx.realInteropHash!, receiverProvider, senderProvider);
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

    afterAll(async () => {
        await testMaster.deinitialize();
    });
});
