import * as fs from 'fs';
import * as path from 'path';

import { TestMaster } from './test-master';
import { Token } from './types';
import * as utils from 'utils';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { encodeNTVAssetId } from 'zksync-ethers/build/utils';

import { RetryableWallet } from './retry-provider';
import {
    scaledGasPrice,
    deployContract,
    readContract,
    waitUntilBlockFinalized,
    waitForInteropRootNonZero,
    getGWBlockNumber,
    formatEvmV1Address,
    formatEvmV1Chain,
    getL2bUrl,
    waitUntilBlockExecutedOnGateway
} from './helpers';

import {
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    L2_INTEROP_CENTER_ADDRESS,
    L2_ASSET_TRACKER_ADDRESS,
    GW_ASSET_TRACKER_ADDRESS,
    ETH_ADDRESS_IN_CONTRACTS,
    ARTIFACTS_PATH,
    ArtifactInteropCenter,
    ArtifactInteropHandler,
    ArtifactL2AssetTracker,
    ArtifactGWAssetTracker,
    ArtifactNativeTokenVault,
    ArtifactMintableERC20,
    ArtifactDummyInteropRecipient,
    ArtifactIERC7786Attributes,
    ArtifactL1BridgeHub
} from './constants';
import { RetryProvider } from './retry-provider';
import { getInteropBundleData } from './temp-sdk';

const SHARED_STATE_FILE = path.join(__dirname, '../interop-shared-state.json');
const LOCK_DIR = path.join(__dirname, '../interop-setup.lock');

export interface InteropCallStarter {
    to: string;
    data: string;
    callAttributes: string[];
}

export interface AssetTrackerBalanceSnapshot {
    senderChainId: bigint;
    receiverChainId: bigint;
    senderBaseTokenAssetId: string;
    receiverBaseTokenAssetId: string;
    tokenAssetId: string;
    gwatSenderBase: bigint;
    gwatSenderToken: bigint;
    gwatReceiverBase: bigint;
    gwatReceiverToken: bigint;
    l2SenderBase: bigint;
    l2ReceiverBase: bigint;
    l2SenderToken: bigint;
    l2ReceiverToken: bigint;
}

export interface AssetTrackerBalanceDeltas {
    gwatSenderBase?: bigint;
    gwatSenderToken?: bigint;
    gwatReceiverBase?: bigint;
    gwatReceiverToken?: bigint;
    l2SenderBase?: bigint;
    l2ReceiverBase?: bigint;
    l2SenderToken?: bigint;
    l2ReceiverToken?: bigint;
}

export class InteropTestContext {
    public testMaster!: TestMaster;
    public mainAccount!: RetryableWallet;
    public mainAccountSecondChain!: RetryableWallet;
    public tokenDetails!: Token;
    public skipInteropTests = false;
    public l1Provider!: ethers.Provider;

    // Token A (native to interop1 L2 chain)
    public tokenA: Token = {
        name: 'Token A',
        symbol: 'AA',
        decimals: 18n,
        assetId: '',
        l1Address: '',
        l2Address: '',
        l2AddressSecondChain: ''
    };

    // Interop1 (Main Chain) Variables
    public baseToken1!: Token;
    public interop1Provider!: zksync.Provider;
    public interop1ChainId!: bigint;
    public interop1Wallet!: zksync.Wallet;
    public interop1RichWallet!: zksync.Wallet;
    public interop1InteropCenter!: zksync.Contract;
    public interop1NativeTokenVault!: zksync.Contract;
    public interop1TokenA!: zksync.Contract;

    // Interop2 (Second Chain) Variables
    public baseToken2!: Token;
    public interop2Recipient!: zksync.Wallet;
    public otherInterop2Recipient!: zksync.Wallet;
    public interop2ChainId!: bigint;
    public interop2RichWallet!: zksync.Wallet;
    public interop2Provider!: zksync.Provider;
    public interop2InteropHandler!: zksync.Contract;
    public interop2NativeTokenVault!: zksync.Contract;
    public dummyInteropRecipient!: string;
    public otherDummyInteropRecipient!: string;

    // Gateway Variables
    public gatewayProvider!: zksync.Provider;
    public gatewayWallet!: zksync.Wallet;
    public gwAssetTracker!: zksync.Contract;
    public interop1AssetTracker!: zksync.Contract;
    public interop2AssetTracker!: zksync.Contract;

    public erc7786AttributeDummy!: zksync.Contract;
    public isSameBaseToken!: boolean;

    constructor() {}

    async initialize(testFilename: string) {
        this.testMaster = TestMaster.getInstance(testFilename);
        this.mainAccount = this.testMaster.mainAccount();
        this.tokenDetails = this.testMaster.environment().erc20Token;

        const testWalletPK = this.testMaster.newEmptyAccount().privateKey;

        // Initialize providers
        this.l1Provider = this.mainAccount.providerL1!;
        this.interop1Provider = this.mainAccount.provider;
        this.interop1ChainId = (await this.interop1Provider.getNetwork()).chainId;

        // Initialize Test Master and create wallets for Interop1
        this.interop1Wallet = new zksync.Wallet(testWalletPK, this.interop1Provider, this.l1Provider);
        this.interop1RichWallet = new zksync.Wallet(
            this.mainAccount.privateKey,
            this.interop1Provider,
            this.l1Provider
        );

        // Skip interop tests if the SL is the same as the L1.
        const bridgehub = new ethers.Contract(
            await this.mainAccount.provider.getBridgehubContractAddress(),
            ArtifactL1BridgeHub.abi,
            this.mainAccount.providerL1
        );

        if (
            (await bridgehub.settlementLayer((await this.mainAccount.provider.getNetwork()).chainId)) ==
            (await this.mainAccount.providerL1!.getNetwork()).chainId
        ) {
            this.skipInteropTests = true;
        } else {
            // Define the second chain wallet if the SL is different from the L1.
            const maybemainAccountSecondChain = this.testMaster.mainAccountSecondChain();
            if (!maybemainAccountSecondChain) {
                throw new Error(
                    'Interop tests cannot be run if the second chain is not set. Use the --second-chain flag to specify a different second chain to run the tests on.'
                );
            }
            this.mainAccountSecondChain = maybemainAccountSecondChain!;
        }

        // Setup Interop2 Provider and Wallet
        if (this.skipInteropTests) {
            return;
        }
        this.interop2Provider = this.mainAccountSecondChain.provider;
        this.interop2ChainId = (await this.interop2Provider.getNetwork()).chainId;
        this.interop2RichWallet = new zksync.Wallet(
            this.mainAccount.privateKey,
            this.interop2Provider,
            this.l1Provider
        );
        this.interop2Recipient = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, this.interop2Provider);
        this.otherInterop2Recipient = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, this.interop2Provider);

        // Setup gateway provider and wallet
        this.gatewayProvider = new RetryProvider(
            { url: await getL2bUrl('gateway'), timeout: 1200 * 1000 },
            undefined,
            this.testMaster.reporter
        );
        this.gatewayWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, this.gatewayProvider);

        // Initialize Contracts on Interop1
        this.interop1InteropCenter = new zksync.Contract(
            L2_INTEROP_CENTER_ADDRESS,
            ArtifactInteropCenter.abi,
            this.interop1Wallet
        );
        this.interop1NativeTokenVault = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            this.interop1Wallet
        );

        // Initialize Contracts on Interop2
        this.interop2InteropHandler = new zksync.Contract(
            L2_INTEROP_HANDLER_ADDRESS,
            ArtifactInteropHandler.abi,
            this.interop2RichWallet
        );
        this.interop2NativeTokenVault = new zksync.Contract(
            L2_NATIVE_TOKEN_VAULT_ADDRESS,
            ArtifactNativeTokenVault.abi,
            this.interop2Provider
        );
        this.interop1AssetTracker = new zksync.Contract(
            L2_ASSET_TRACKER_ADDRESS,
            ArtifactL2AssetTracker.abi,
            this.interop1Wallet
        );
        this.interop2AssetTracker = new zksync.Contract(
            L2_ASSET_TRACKER_ADDRESS,
            ArtifactL2AssetTracker.abi,
            this.interop2RichWallet
        );
        this.gwAssetTracker = new zksync.Contract(
            GW_ASSET_TRACKER_ADDRESS,
            ArtifactGWAssetTracker.abi,
            this.gatewayWallet
        );

        // Deposit funds on Interop1
        const gasPrice = await scaledGasPrice(this.interop1RichWallet);
        this.baseToken1 = this.testMaster.environment().baseToken;
        this.baseToken1.assetId = await this.interop1NativeTokenVault.assetId(this.baseToken1.l2Address);
        this.baseToken2 = this.testMaster.environment().baseTokenSecondChain!;
        this.baseToken2.assetId = await this.interop2NativeTokenVault.assetId(this.baseToken2.l2Address);

        await (
            await this.interop1RichWallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount: ethers.parseEther('0.1'),
                to: this.interop1Wallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            })
        ).wait();
        if (this.baseToken1.l1Address != ETH_ADDRESS_IN_CONTRACTS) {
            const depositTx = await this.interop1RichWallet.deposit({
                token: this.baseToken1.l1Address,
                amount: ethers.parseEther('0.1'),
                to: this.interop1Wallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            });
            await depositTx.wait();
        }

        if (this.baseToken2.l1Address != this.baseToken1.l1Address) {
            const depositTx = await this.interop1RichWallet.deposit({
                token: this.baseToken2.l1Address,
                amount: ethers.parseEther('0.1'),
                to: this.interop1Wallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            });
            await depositTx.wait();
            this.baseToken2.l2AddressSecondChain = await this.interop1NativeTokenVault.tokenAddress(
                this.baseToken2.assetId
            );
        }

        // Deposit funds on Interop2
        await (
            await this.interop2RichWallet.deposit({
                token: ETH_ADDRESS_IN_CONTRACTS,
                amount: ethers.parseEther('0.1'),
                to: this.interop2RichWallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            })
        ).wait();

        if (this.baseToken2.l1Address != ETH_ADDRESS_IN_CONTRACTS) {
            const depositTx = await this.interop2RichWallet.deposit({
                token: this.baseToken2.l1Address,
                amount: ethers.parseEther('0.1'),
                to: this.interop2RichWallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            });
            await depositTx.wait();
        }

        if (this.baseToken1.l1Address != this.baseToken2.l1Address) {
            const depositTx = await this.interop2RichWallet.deposit({
                token: this.baseToken1.l1Address,
                amount: ethers.parseEther('0.1'),
                to: this.interop2RichWallet.address,
                approveERC20: true,
                approveBaseERC20: true,
                approveOverrides: { gasPrice },
                overrides: { gasPrice }
            });
            await depositTx.wait();
        }

        // Define dummy interop recipients
        const dummyInteropRecipientContract = await deployContract(
            this.interop2RichWallet,
            ArtifactDummyInteropRecipient,
            []
        );
        this.dummyInteropRecipient = await dummyInteropRecipientContract.getAddress();
        const otherDummyInteropRecipientContract = await deployContract(
            this.interop2RichWallet,
            ArtifactDummyInteropRecipient,
            []
        );
        this.otherDummyInteropRecipient = await otherDummyInteropRecipientContract.getAddress();

        this.erc7786AttributeDummy = new zksync.Contract(
            '0x0000000000000000000000000000000000000000',
            ArtifactIERC7786Attributes.abi,
            this.interop1Wallet
        );

        this.isSameBaseToken =
            this.testMaster.environment().baseToken.l1Address ==
            this.testMaster.environment().baseTokenSecondChain!.l1Address;
    }

    /**
     * Performs a one-time setup for interop tests
     */
    async performSharedSetup() {
        if (this.skipInteropTests) return;

        // Skip locking for local single-suite runs
        if (process.env.INTEROP_SKIP_LOCK === 'true') {
            console.log(`[${process.pid}] Skipping lock (INTEROP_SKIP_LOCK=true), performing setup directly`);
            await this.performSetup();
            return;
        }

        const maxRetries = 300; // Wait up to 300 seconds
        let hasLock = false;

        // 1. Attempt to acquire lock or wait for state file
        for (let i = 0; i < maxRetries; i++) {
            // Check if setup is already complete by another process
            if (fs.existsSync(SHARED_STATE_FILE)) {
                try {
                    // Small delay to ensure writer has finished flushing file content
                    if (i === 0) await utils.sleep(1);

                    const state = JSON.parse(fs.readFileSync(SHARED_STATE_FILE, 'utf-8'));
                    this.loadState(state);
                    await this.fundInterop1TokenAForSuite();
                    return;
                } catch (e) {
                    // File might be half-written, continue waiting
                }
            }

            // Try to acquire lock
            try {
                fs.mkdirSync(LOCK_DIR);
                hasLock = true;
                break; // We have the lock, proceed to setup
            } catch (err: any) {
                if (err.code === 'EEXIST') {
                    // Lock exists, wait and retry
                    await utils.sleep(1);
                } else {
                    throw err;
                }
            }
        }

        if (!hasLock) {
            throw new Error(`[${process.pid}] Timed out waiting for interop shared setup.`);
        }

        // 2. Perform Setup
        try {
            await this.performSetup();
        } catch (error) {
            console.error(`[${process.pid}] Setup failed, removing lock.`);
            // If we fail, remove lock so others might try (or fail faster)
            try {
                fs.rmdirSync(LOCK_DIR);
            } catch (_) {}
            // Also remove partial state file if it exists
            if (fs.existsSync(SHARED_STATE_FILE)) fs.unlinkSync(SHARED_STATE_FILE);
            throw error;
        } finally {
            // 3. Release Lock
            if (hasLock) {
                try {
                    fs.rmdirSync(LOCK_DIR);
                } catch (e) {
                    console.warn(`[${process.pid}] Failed to release lock:`, e);
                }
            }
        }
    }

    private async performSetup() {
        const l1TokenArtifact = readContract(ARTIFACTS_PATH, 'TestnetERC20Token');
        const l1TokenBytecode = l1TokenArtifact.bytecode.object ?? l1TokenArtifact.bytecode;
        const l1TokenFactory = new ethers.ContractFactory(
            l1TokenArtifact.abi,
            l1TokenBytecode,
            this.interop1RichWallet.ethWallet()
        );
        const l1TokenDeploy = await l1TokenFactory.deploy(
            this.tokenA.name,
            this.tokenA.symbol,
            Number(this.tokenA.decimals)
        );
        await l1TokenDeploy.waitForDeployment();
        this.tokenA.l1Address = await l1TokenDeploy.getAddress();
        await this.fundInterop1TokenAForSuite();

        const l1ChainId = (await this.l1Provider.getNetwork()).chainId;
        this.tokenA.assetId = encodeNTVAssetId(l1ChainId, this.tokenA.l1Address);
        while (true) {
            this.tokenA.l2Address = await this.interop1NativeTokenVault.tokenAddress(this.tokenA.assetId);
            if (this.tokenA.l2Address !== ethers.ZeroAddress) break;
            await utils.sleep(1);
        }
        this.interop1TokenA = new zksync.Contract(
            this.tokenA.l2Address,
            ArtifactMintableERC20.abi,
            this.interop1Wallet
        );

        // Save State
        const newState = {
            tokenA: {
                name: this.tokenA.name,
                symbol: this.tokenA.symbol,
                l1Address: this.tokenA.l1Address,
                l2Address: this.tokenA.l2Address,
                l2AddressSecondChain: this.tokenA.l2AddressSecondChain,
                assetId: this.tokenA.assetId
            }
        };

        this.loadState(newState);
        fs.writeFileSync(SHARED_STATE_FILE, JSON.stringify(newState, null, 2));
    }

    private async fundInterop1TokenAForSuite() {
        const l1Token = new ethers.Contract(
            this.tokenA.l1Address,
            readContract(ARTIFACTS_PATH, 'TestnetERC20Token').abi,
            this.interop1RichWallet.ethWallet()
        );
        const initialL1Amount = ethers.parseEther('1000');
        await (await l1Token.mint(this.interop1RichWallet.address, initialL1Amount)).wait();
        await (
            await this.interop1RichWallet.deposit({
                token: this.tokenA.l1Address,
                amount: initialL1Amount,
                to: this.interop1Wallet.address,
                approveERC20: true,
                approveBaseERC20: true
            })
        ).wait();
    }

    private loadState(state: any) {
        this.tokenA = {
            ...state.tokenA,
            decimals: 18n // Default value, not used in this test suite anyway
        };

        this.interop1TokenA = new zksync.Contract(
            this.tokenA.l2Address,
            ArtifactMintableERC20.abi,
            this.interop1Wallet
        );
    }

    async deinitialize() {
        if (this.testMaster) {
            await this.testMaster.deinitialize();
        }
    }

    /// HELPER FUNCTIONS

    /**
     *  Helper to create the standard execution address attribute
     */
    async executionAddressAttr(executionAddress: string = this.interop2RichWallet.address) {
        return this.erc7786AttributeDummy.interface.encodeFunctionData('executionAddress', [
            formatEvmV1Address(executionAddress, this.interop2ChainId)
        ]);
    }

    /**
     * Helper to create attributes with interopCallValue
     */
    async directCallAttrs(amount: bigint, executionAddress?: string) {
        return [
            await this.erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [amount]),
            await this.executionAddressAttr(executionAddress)
        ];
    }

    /**
     * Helper to create attributes with indirectCall
     */
    async indirectCallAttrs(callValue: bigint = 0n, executionAddress?: string) {
        return [
            await this.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [callValue]),
            await this.executionAddressAttr(executionAddress)
        ];
    }

    async indirectDirectCallAttrs(amount: bigint, callValue: bigint = 0n, executionAddress?: string) {
        return [
            await this.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [callValue]),
            await this.erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [amount]),
            await this.executionAddressAttr(executionAddress)
        ];
    }

    /**
     * Helper to encode interopCallValue attribute (for bundle call attributes)
     */
    interopCallValueAttr(amount: bigint): string {
        return this.erc7786AttributeDummy.interface.encodeFunctionData('interopCallValue', [amount]);
    }

    /**
     * Helper to encode indirectCall attribute (for bundle call attributes)
     */
    indirectCallAttr(callValue: bigint = 0n): string {
        return this.erc7786AttributeDummy.interface.encodeFunctionData('indirectCall', [callValue]);
    }

    /**
     * Sends a direct L2 transaction request on Interop1.
     * The function prepares the interop call input and populates the transaction before sending.
     */
    async fromInterop1RequestInterop(
        execCallStarters: InteropCallStarter[],
        bundleOptions?: { executionAddress?: string; unbundlerAddress?: string },
        overrides: ethers.Overrides = {}
    ) {
        const bundleAttributes = [];
        if (bundleOptions?.executionAddress) {
            bundleAttributes.push(
                await this.erc7786AttributeDummy.interface.encodeFunctionData('executionAddress', [
                    formatEvmV1Address(bundleOptions.executionAddress, this.interop2ChainId)
                ])
            );
        }
        if (bundleOptions?.unbundlerAddress) {
            bundleAttributes.push(
                await this.erc7786AttributeDummy.interface.encodeFunctionData('unbundlerAddress', [
                    formatEvmV1Address(bundleOptions.unbundlerAddress, this.interop2ChainId)
                ])
            );
        }

        const txFinalizeReceipt = (
            await this.interop1InteropCenter.sendBundle(
                formatEvmV1Chain((await this.interop2Provider.getNetwork()).chainId),
                execCallStarters,
                bundleAttributes,
                overrides
            )
        ).wait();
        return txFinalizeReceipt;
    }

    /**
     * Generates ABI-encoded data for transferring tokens using the second bridge.
     */
    getTokenTransferSecondBridgeData(assetId: string, amount: bigint, recipient: string) {
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
     * Waits for an interop bundle to be executable on the receiver chain.
     */
    async awaitInteropBundle(txHash: string) {
        const senderUtilityWallet = new zksync.Wallet(zksync.Wallet.createRandom().privateKey, this.interop1Provider);
        const txReceipt = await this.interop1Provider.getTransactionReceipt(txHash);
        await waitUntilBlockFinalized(senderUtilityWallet, txReceipt!.blockNumber);

        await waitUntilBlockExecutedOnGateway(senderUtilityWallet, this.gatewayWallet, txReceipt!.blockNumber);
        await utils.sleep(1); // Additional delay to avoid flakiness
        const params = await senderUtilityWallet.getFinalizeWithdrawalParams(txHash, 0, 'proof_based_gw');
        await waitForInteropRootNonZero(this.interop2Provider, this.interop2RichWallet, getGWBlockNumber(params));
    }

    /**
     * Reads an interop transaction from the sender chain, constructs a new transaction,
     * and broadcasts it on the receiver chain.
     */
    async readAndBroadcastInteropBundle(txHash: string): Promise<ethers.TransactionReceipt | undefined> {
        // Get interop trigger and bundle data from the sender chain.
        const executionBundle = await getInteropBundleData(this.interop1Provider, txHash, 0);
        if (executionBundle.output == null) return undefined;

        const tx = await this.interop2InteropHandler.executeBundle(
            executionBundle.rawData,
            executionBundle.proofDecoded
        );
        return await tx.wait();
    }

    async getGatewayChainBalance(chainId: bigint, assetId: string): Promise<bigint> {
        return await this.gwAssetTracker.chainBalance(chainId, assetId);
    }

    async getInterop1ChainBalance(assetId: string): Promise<bigint> {
        return await this.interop1AssetTracker.chainBalance(this.interop1ChainId, assetId);
    }

    async getInterop2ChainBalance(assetId: string): Promise<bigint> {
        return await this.interop2AssetTracker.chainBalance(this.interop2ChainId, assetId);
    }

    async waitUntilInterop2BlockExecutedOnGateway(blockNumber: number): Promise<void> {
        await waitUntilBlockExecutedOnGateway(this.interop2RichWallet, this.gatewayWallet, blockNumber);
    }

    async snapshotAssetTrackerBalances(
        senderChainId: bigint,
        receiverChainId: bigint,
        senderBaseTokenAssetId: string,
        receiverBaseTokenAssetId: string,
        tokenAssetId: string
    ): Promise<AssetTrackerBalanceSnapshot> {
        if (!senderBaseTokenAssetId || !receiverBaseTokenAssetId || !tokenAssetId) {
            throw new Error('Missing asset ID for base token or Token A');
        }

        const [gwatSenderBase, gwatSenderToken, gwatReceiverBase, gwatReceiverToken] = await Promise.all([
            this.getGatewayChainBalance(senderChainId, senderBaseTokenAssetId),
            this.getGatewayChainBalance(senderChainId, tokenAssetId),
            this.getGatewayChainBalance(receiverChainId, receiverBaseTokenAssetId),
            this.getGatewayChainBalance(receiverChainId, tokenAssetId)
        ]);

        const [l2SenderBase, l2ReceiverBase, l2SenderToken, l2ReceiverToken] = await Promise.all([
            this.getInterop1ChainBalance(senderBaseTokenAssetId),
            this.getInterop2ChainBalance(receiverBaseTokenAssetId),
            this.getInterop1ChainBalance(tokenAssetId),
            this.getInterop2ChainBalance(tokenAssetId)
        ]);

        return {
            senderChainId,
            receiverChainId,
            senderBaseTokenAssetId,
            receiverBaseTokenAssetId,
            tokenAssetId,
            gwatSenderBase,
            gwatSenderToken,
            gwatReceiverBase,
            gwatReceiverToken,
            l2SenderBase,
            l2ReceiverBase,
            l2SenderToken,
            l2ReceiverToken
        };
    }

    async snapshotInteropAssetTrackerBalances(): Promise<AssetTrackerBalanceSnapshot> {
        if (!this.baseToken1.assetId || !this.baseToken2.assetId || !this.tokenA.assetId) {
            throw new Error('Missing asset ID for base token or Token A');
        }

        return await this.snapshotAssetTrackerBalances(
            this.interop1ChainId,
            this.interop2ChainId,
            this.baseToken1.assetId,
            this.baseToken2.assetId,
            this.tokenA.assetId
        );
    }

    async assertAssetTrackerBalances(
        snapshot: AssetTrackerBalanceSnapshot,
        deltas: AssetTrackerBalanceDeltas = {}
    ): Promise<void> {
        const current = await this.snapshotAssetTrackerBalances(
            snapshot.senderChainId,
            snapshot.receiverChainId,
            snapshot.senderBaseTokenAssetId,
            snapshot.receiverBaseTokenAssetId,
            snapshot.tokenAssetId
        );

        const expectedGwatSenderBase = snapshot.gwatSenderBase + (deltas.gwatSenderBase ?? 0n);
        const expectedGwatSenderToken = snapshot.gwatSenderToken + (deltas.gwatSenderToken ?? 0n);
        const expectedGwatReceiverBase = snapshot.gwatReceiverBase + (deltas.gwatReceiverBase ?? 0n);
        const expectedGwatReceiverToken = snapshot.gwatReceiverToken + (deltas.gwatReceiverToken ?? 0n);

        if (current.gwatSenderBase !== expectedGwatSenderBase) {
            throw new Error(
                `GWAT sender base balance mismatch (chainId=${snapshot.senderChainId}, assetId=${snapshot.senderBaseTokenAssetId}): expected ${expectedGwatSenderBase}, got ${current.gwatSenderBase}`
            );
        }
        if (current.gwatSenderToken !== expectedGwatSenderToken) {
            throw new Error(
                `GWAT sender token balance mismatch (chainId=${snapshot.senderChainId}, assetId=${snapshot.tokenAssetId}): expected ${expectedGwatSenderToken}, got ${current.gwatSenderToken}`
            );
        }
        if (current.gwatReceiverBase !== expectedGwatReceiverBase) {
            throw new Error(
                `GWAT receiver base balance mismatch (chainId=${snapshot.receiverChainId}, assetId=${snapshot.receiverBaseTokenAssetId}): expected ${expectedGwatReceiverBase}, got ${current.gwatReceiverBase}`
            );
        }
        if (current.gwatReceiverToken !== expectedGwatReceiverToken) {
            throw new Error(
                `GWAT receiver token balance mismatch (chainId=${snapshot.receiverChainId}, assetId=${snapshot.tokenAssetId}): expected ${expectedGwatReceiverToken}, got ${current.gwatReceiverToken}`
            );
        }

        const expectedL2SenderBase = snapshot.l2SenderBase + (deltas.l2SenderBase ?? 0n);
        const expectedL2ReceiverBase = snapshot.l2ReceiverBase + (deltas.l2ReceiverBase ?? 0n);

        if (current.l2SenderBase !== expectedL2SenderBase) {
            throw new Error(
                `L2AssetTracker sender base balance mismatch (chainId=${snapshot.senderChainId}, assetId=${snapshot.senderBaseTokenAssetId}): expected ${expectedL2SenderBase}, got ${current.l2SenderBase}`
            );
        }
        if (current.l2ReceiverBase !== expectedL2ReceiverBase) {
            throw new Error(
                `L2AssetTracker receiver base balance mismatch (chainId=${snapshot.receiverChainId}, assetId=${snapshot.receiverBaseTokenAssetId}): expected ${expectedL2ReceiverBase}, got ${current.l2ReceiverBase}`
            );
        }

        const expectedL2SenderToken = snapshot.l2SenderToken + (deltas.l2SenderToken ?? 0n);
        const expectedL2ReceiverToken = snapshot.l2ReceiverToken + (deltas.l2ReceiverToken ?? 0n);

        if (current.l2SenderToken !== expectedL2SenderToken) {
            throw new Error(
                `L2AssetTracker sender token balance mismatch (chainId=${snapshot.senderChainId}, assetId=${snapshot.tokenAssetId}): expected ${expectedL2SenderToken}, got ${current.l2SenderToken}`
            );
        }
        if (current.l2ReceiverToken !== expectedL2ReceiverToken) {
            throw new Error(
                `L2AssetTracker receiver token balance mismatch (chainId=${snapshot.receiverChainId}, assetId=${snapshot.tokenAssetId}): expected ${expectedL2ReceiverToken}, got ${current.l2ReceiverToken}`
            );
        }
    }

    /**
     * Retrieves the token balance for a given wallet or address.
     */
    async getTokenBalance(
        walletOrAddress: zksync.Wallet | string,
        tokenAddress: string,
        explicitProvider?: zksync.Provider
    ): Promise<bigint> {
        if (!tokenAddress) throw new Error('Token address is not provided');
        // Happens when token wasn't deployed yet. Therefore there is no balance.
        if (tokenAddress === ethers.ZeroAddress) return 0n;

        const address = typeof walletOrAddress === 'string' ? walletOrAddress : walletOrAddress.address;
        const provider = typeof walletOrAddress === 'string' ? explicitProvider! : walletOrAddress.provider!;

        const tokenContract = new zksync.Contract(tokenAddress, ArtifactMintableERC20.abi, provider);
        const balance = await tokenContract.balanceOf(address);
        return balance;
    }

    /**
     * Retrieves the address' balance on Chain B.
     */
    async getInterop2Balance(address: string): Promise<bigint> {
        return BigInt(await this.interop2Provider.getBalance(address));
    }

    /**
     * Returns a random amount to transfer.
     */
    getTransferAmount(): bigint {
        return ethers.parseUnits((Math.floor(Math.random() * 900) + 100).toString(), 'gwei');
    }
}
