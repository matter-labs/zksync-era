/**
 * This suite contains tests checking interop functionality.
 */

import { TestMaster } from '../src';
import { Token } from '../src/types';
import * as utils from 'utils';
import { shouldLoadConfigFromFile } from 'utils/build/file-configs';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';

import { RetryableWallet } from '../src/retry-provider';
import {
    waitForL2ToL1LogProof,
    scaledGasPrice,
    deployContract,
    waitUntilBlockFinalized,
    waitForInteropRootNonZero,
    getGWBlockNumber,
    formatEvmV1Address,
    formatEvmV1Chain,
    getL2bUrl
} from '../src/helpers';

import {
    L2_ASSET_ROUTER_ADDRESS,
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    L2_INTEROP_CENTER_ADDRESS,
    L2_MESSAGE_VERIFICATION_ADDRESS,
    ETH_ADDRESS_IN_CONTRACTS,
    ArtifactInteropCenter,
    ArtifactInteropHandler,
    ArtifactNativeTokenVault,
    ArtifactMintableERC20,
    ArtifactDummyInteropRecipient,
    ArtifactIERC7786Attributes,
    ArtifactL2MessageVerification,
    ArtifactL1BridgeHub
} from '../src/constants';
import { RetryProvider } from '../src/retry-provider';
import { getInteropBundleData } from '../src/temp-sdk';

describe('Interop behavior checks', () => {
    let testMaster: TestMaster;
    let mainAccount: RetryableWallet;
    let mainAccountSecondChain: RetryableWallet;
    let tokenDetails: Token;

    let skipInteropTests = false;

    // L1 Variables
    let l1Provider: ethers.Provider;

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
    let interop1NativeTokenVault: zksync.Contract;
    let interop1TokenA: zksync.Contract;

    // Interop2 (Second Chain) Variables
    let interop2RichWallet: zksync.Wallet;
    let interop2Provider: zksync.Provider;
    let interop2InteropHandler: zksync.Contract;
    let interop2NativeTokenVault: zksync.Contract;
    let dummyInteropRecipient: string;

    // Gateway Variables
    let gatewayProvider: zksync.Provider;
    let gatewayWallet: zksync.Wallet;

    let isSameBaseToken: boolean;

    beforeAll(async () => {
        testMaster = TestMaster.getInstance(__filename);
        mainAccount = testMaster.mainAccount();
        tokenDetails = testMaster.environment().erc20Token;

        const testWalletPK = testMaster.newEmptyAccount().privateKey;

        // Initialize providers
        l1Provider = mainAccount.providerL1!;
        interop1Provider = mainAccount.provider;

        // Initialize Test Master and create wallets for Interop1
        interop1Wallet = new zksync.Wallet(testWalletPK, interop1Provider, l1Provider);
        interop1RichWallet = new zksync.Wallet(mainAccount.privateKey, interop1Provider, l1Provider);

        // Skip interop tests if the SL is the same as the L1.
        const bridgehub = new ethers.Contract(
            await mainAccount.provider.getBridgehubContractAddress(),
            ArtifactL1BridgeHub.abi,
            mainAccount.providerL1
        );

        if (
            (await bridgehub.settlementLayer((await mainAccount.provider.getNetwork()).chainId)) ==
            (await mainAccount.providerL1!.getNetwork()).chainId
        ) {
            skipInteropTests = true;
        } else {
            // Define the second chain wallet if the SL is different from the L1.
            const maybemainAccountSecondChain = testMaster.mainAccountSecondChain();
            if (!maybemainAccountSecondChain) {
                throw new Error(
                    'Interop tests cannot be run if the second chain is not set. Use the --second-chain flag to specify a different second chain to run the tests on.'
                );
            }
            mainAccountSecondChain = maybemainAccountSecondChain!;
        }

        // Setup Interop2 Provider and Wallet
        if (skipInteropTests) {
            return;
        }
        interop2Provider = mainAccountSecondChain.provider;
        interop2RichWallet = new zksync.Wallet(mainAccount.privateKey, interop2Provider, l1Provider);

        // Setup gateway provider and wallet
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
        interop1NativeTokenVault = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            interop1Wallet
        );

        // Initialize Contracts on Interop2
        interop2InteropHandler = new zksync.Contract(
            L2_INTEROP_HANDLER_ADDRESS,
            ArtifactInteropHandler.abi,
            interop2RichWallet
        );
        interop2NativeTokenVault = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            interop2Provider
        );

        // Deposit funds on Interop1
        const gasPrice = await scaledGasPrice(interop1RichWallet);
        await (
            await interop1RichWallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount: ethers.parseEther('0.1'),
                to: interop1Wallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            })
        ).wait();
        if (testMaster.environment().baseToken.l1Address != ETH_ADDRESS_IN_CONTRACTS) {
            const depositTx = await interop1RichWallet.deposit({
                token: testMaster.environment().baseToken.l1Address,
                amount: ethers.parseEther('0.1'),
                to: interop1Wallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            });
            await depositTx.wait();
        }

        // Deposit funds on Interop2
        await (
            await interop2RichWallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount: ethers.parseEther('0.1'),
                to: interop2RichWallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            })
        ).wait();

        isSameBaseToken =
            testMaster.environment().baseToken.l1Address == testMaster.environment().baseTokenSecondChain!.l1Address;
    });

    let withdrawalHash: string;
    let params: zksync.types.FinalizeWithdrawalParams;
    test('Can check withdrawal hash in L2-A', async () => {
        if (skipInteropTests) {
            return;
        }

        const l2MessageVerification = new zksync.Contract(
            L2_MESSAGE_VERIFICATION_ADDRESS,
            ArtifactL2MessageVerification.abi,
            mainAccount.provider
        );

        // Perform a withdrawal and wait for it to be processed
        const withdrawalPromise = await mainAccount.withdraw({
            token: tokenDetails.l2Address,
            amount: 1
        });
        await expect(withdrawalPromise).toBeAccepted();
        const withdrawalTx = await withdrawalPromise;
        withdrawalHash = withdrawalTx.hash;
        const l2TxReceipt = await mainAccount.provider.getTransactionReceipt(withdrawalHash);
        await waitForL2ToL1LogProof(mainAccount, l2TxReceipt!.blockNumber, withdrawalHash);

        // Proof-based interop on Gateway, meaning the Merkle proof hashes to Gateway's MessageRoot
        params = await mainAccount.getFinalizeWithdrawalParams(withdrawalHash, undefined, 'proof_based_gw');

        // Needed else the L2's view of GW's MessageRoot won't be updated
        await waitForInteropRootNonZero(mainAccount.provider, mainAccount, getGWBlockNumber(params));

        const included = await l2MessageVerification.proveL2MessageInclusionShared(
            Number((await mainAccount.provider.getNetwork()).chainId),
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
            mainAccountSecondChain.provider
        );

        // Needed else the L2's view of GW's MessageRoot won't be updated
        await waitForInteropRootNonZero(
            mainAccountSecondChain.provider,
            mainAccountSecondChain,
            getGWBlockNumber(params)
        );

        // We use the same proof that was verified in L2-A
        const included = await l2MessageVerification.proveL2MessageInclusionShared(
            Number((await mainAccount.provider.getNetwork()).chainId),
            params.l1BatchNumber,
            params.l2MessageIndex,
            { txNumberInBatch: params.l2TxNumberInBlock, sender: params.sender, data: params.message },
            params.proof
        );
        expect(included).toBe(true);
    });

    test('Can register and migrate a token', async () => {
        if (skipInteropTests) {
            return;
        }

        // Deploy Token A on Interop1
        const tokenADeploy = await deployContract(interop1Wallet, ArtifactMintableERC20, [
            tokenA.name,
            tokenA.symbol,
            tokenA.decimals
        ]);
        const dummyInteropRecipientContract = await deployContract(
            interop2RichWallet,
            ArtifactDummyInteropRecipient,
            []
        );

        dummyInteropRecipient = await dummyInteropRecipientContract.getAddress();
        tokenA.l2Address = await tokenADeploy.getAddress();

        // Register Token A
        await (await interop1NativeTokenVault.registerToken(tokenA.l2Address)).wait();
        tokenA.assetId = await interop1NativeTokenVault.assetId(tokenA.l2Address);
        interop1TokenA = new zksync.Contract(tokenA.l2Address, ArtifactMintableERC20.abi, interop1Wallet);

        const fileConfig = shouldLoadConfigFromFile();
        await utils.spawn(
            `zkstack chain gateway migrate-token-balances --to-gateway true --gateway-chain-name gateway --chain ${fileConfig.chain}`
        );
    });

    test('Can perform cross chain transfer', async () => {
        if (skipInteropTests) {
            return;
        }
        const transferAmount = 100n;

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

        const receipt = await fromInterop1RequestInterop(
            // Execution call starters for token transfer
            [
                {
                    to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                    data: getTokenTransferSecondBridgeData(tokenA.assetId!, transferAmount, interop2RichWallet.address),
                    callAttributes: [await erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0])]
                }
            ]
        );

        // Broadcast interop transaction from Interop1 to Interop2
        await readAndBroadcastInteropBundle(receipt.hash, interop1Provider, interop2Provider);

        tokenA.l2AddressSecondChain = await interop2NativeTokenVault.tokenAddress(tokenA.assetId);

        // Check the token balance on the second chain
        const interop1WalletSecondChainBalance = await getTokenBalance({
            provider: interop2Provider,
            tokenAddress: tokenA.l2AddressSecondChain!,
            address: interop2RichWallet.address
        });
        expect(interop1WalletSecondChainBalance.toString()).toBe(transferAmount.toString());
    });

    test('Can perform cross chain bundle', async () => {
        if (skipInteropTests) {
            return;
        }
        const transferAmount = 100n;

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

        const receipt = await fromInterop1RequestInterop(
            // Execution call starters for token transfer
            [
                {
                    to: formatEvmV1Address(L2_ASSET_ROUTER_ADDRESS),
                    data: getTokenTransferSecondBridgeData(tokenA.assetId!, transferAmount, interop2RichWallet.address),
                    callAttributes: [await erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [0])]
                },
                {
                    to: formatEvmV1Address(dummyInteropRecipient),
                    data: '0x',
                    callAttributes: [
                        await erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [transferAmount])
                    ]
                }
            ],
            { value: isSameBaseToken ? transferAmount : 0n }
        );

        // Broadcast interop transaction from Interop1 to Interop2
        await readAndBroadcastInteropBundle(receipt.hash, interop1Provider, interop2Provider);

        tokenA.l2AddressSecondChain = await interop2NativeTokenVault.tokenAddress(tokenA.assetId);

        // Check the token balance on the second chain
        const interop1WalletSecondChainBalance = await getTokenBalance({
            provider: interop2Provider,
            tokenAddress: tokenA.l2AddressSecondChain!,
            address: interop2RichWallet.address
        });
        expect(interop1WalletSecondChainBalance.toString()).toBe((2n * transferAmount).toString());
    });

    // Types for interop call starters and gas fields.
    interface InteropCallStarter {
        to: string; // ERC-7930 formatted address bytes
        data: string;
        callAttributes: string[];
    }

    /**
     * Sends a direct L2 transaction request on Interop1.
     * The function prepares the interop call input and populates the transaction before sending.
     */
    async function fromInterop1RequestInterop(
        //feeCallStarters: InteropCallStarter[], // TODO: v32, skipping feeCallStarters for now
        execCallStarters: InteropCallStarter[],
        overrides: ethers.Overrides = {}
    ) {
        const txFinalizeReceipt = (
            await interop1InteropCenter.sendBundle(
                formatEvmV1Chain((await interop2Provider.getNetwork()).chainId),
                execCallStarters,
                [],
                overrides
            )
        ).wait();
        return txFinalizeReceipt;
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
    async function readAndBroadcastInteropBundle(
        txHash: string,
        senderProvider: zksync.Provider,
        receiverProvider: zksync.Provider
    ) {
        const senderUtilityWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, senderProvider);
        const txReceipt = await senderProvider.getTransactionReceipt(txHash);
        await waitUntilBlockFinalized(senderUtilityWallet, txReceipt!.blockNumber);
        // await waitUntilBlockExecutedOnGateway(senderUtilityWallet, gatewayWallet, txReceipt!.blockNumber);
        /// kl todo figure out what we need to wait for here. Probably the fact that we need to wait for the GW block finalization.
        await utils.sleep(25000);
        const params = await senderUtilityWallet.getFinalizeWithdrawalParams(txHash, 0, 'proof_based_gw');
        await waitForInteropRootNonZero(interop2Provider, interop2RichWallet, getGWBlockNumber(params));

        // Get interop trigger and bundle data from the sender chain.
        const executionBundle = await getInteropBundleData(senderProvider, txHash, 0);
        if (executionBundle.output == null) return;

        const receipt = await interop2InteropHandler.executeBundle(
            executionBundle.rawData,
            executionBundle.proofDecoded
        );
        await receipt.wait();
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
