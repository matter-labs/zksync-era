import * as fs from 'fs';
import * as path from 'path';

import { TestMaster } from './test-master';
import { Token } from './types';
import * as utils from 'utils';
import { shouldLoadConfigFromFile } from 'utils/build/file-configs';

import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';

import { RetryableWallet } from './retry-provider';
import {
    scaledGasPrice,
    deployContract,
    waitUntilBlockFinalized,
    waitForInteropRootNonZero,
    getGWBlockNumber,
    formatEvmV1Address,
    formatEvmV1Chain,
    getL2bUrl
} from './helpers';

import {
    L2_NATIVE_TOKEN_VAULT_ADDRESS,
    L2_INTEROP_HANDLER_ADDRESS,
    L2_INTEROP_CENTER_ADDRESS,
    ETH_ADDRESS_IN_CONTRACTS,
    ArtifactInteropCenter,
    ArtifactInteropHandler,
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

    // Token B (bridged from L1)
    public bridgedToken!: Token;

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

        // Define bridged token
        this.bridgedToken = this.testMaster.environment().erc20Token;
        this.bridgedToken.assetId = await this.interop1NativeTokenVault.assetId(this.bridgedToken.l2Address);

        // Deposit funds on Interop1
        const gasPrice = await scaledGasPrice(this.interop1RichWallet);
        this.baseToken1 = this.testMaster.environment().baseToken;
        this.baseToken1.assetId = await this.interop1NativeTokenVault.assetId(this.baseToken1.l2Address);
        this.baseToken2 = this.testMaster.environment().baseTokenSecondChain!;
        this.baseToken2.assetId = await this.interop2NativeTokenVault.assetId(this.baseToken2.l2Address);
        this.baseToken2.l2AddressSecondChain = await this.interop1NativeTokenVault.tokenAddress(
            this.baseToken2.assetId
        );

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
        const tokenADeploy = await deployContract(this.interop1Wallet, ArtifactMintableERC20, [
            this.tokenA.name,
            this.tokenA.symbol,
            this.tokenA.decimals
        ]);
        this.tokenA.l2Address = await tokenADeploy.getAddress();

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

        // Register tokens on Interop1
        await (await this.interop1NativeTokenVault.registerToken(this.tokenA.l2Address)).wait();
        this.tokenA.assetId = await this.interop1NativeTokenVault.assetId(this.tokenA.l2Address);
        this.interop1TokenA = new zksync.Contract(
            this.tokenA.l2Address,
            ArtifactMintableERC20.abi,
            this.interop1Wallet
        );

        const fileConfig = shouldLoadConfigFromFile();
        const migrationCmd = `zkstack chain gateway migrate-token-balances --to-gateway true --gateway-chain-name gateway --chain ${fileConfig.chain}`;

        // Migration might sometimes fail, so we retry a few times.
        for (let attempt = 1; attempt <= 3; attempt++) {
            try {
                await utils.spawn(migrationCmd);
                break;
            } catch (e) {
                if (attempt === 3) throw e;
                await utils.sleep(2 * attempt);
            }
        }

        // Save State
        const newState = {
            tokenA: {
                name: this.tokenA.name,
                symbol: this.tokenA.symbol,
                l1Address: this.tokenA.l1Address,
                l2Address: this.tokenA.l2Address,
                l2AddressSecondChain: this.tokenA.l2AddressSecondChain,
                assetId: this.tokenA.assetId
            },
            dummyRecipientAddress: this.dummyInteropRecipient,
            otherDummyRecipientAddress: this.otherDummyInteropRecipient
        };

        this.loadState(newState);
        fs.writeFileSync(SHARED_STATE_FILE, JSON.stringify(newState, null, 2));
    }

    private loadState(state: any) {
        this.tokenA = {
            ...state.tokenA,
            decimals: 18n // Default value, not used in this test suite anyway
        };
        this.dummyInteropRecipient = state.dummyRecipientAddress;
        this.otherDummyInteropRecipient = state.otherDummyRecipientAddress;

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
     * Sends a direct L2 transaction request on Interop1.
     * The function prepares the interop call input and populates the transaction before sending.
     */
    async fromInterop1RequestInterop(
        execCallStarters: InteropCallStarter[],
        bundleOptions: { executionAddress?: string; unbundlerAddress?: string },
        overrides: ethers.Overrides = {}
    ) {
        const bundleAttributes = [];
        if (bundleOptions.executionAddress) {
            bundleAttributes.push(
                await this.erc7786AttributeDummy.interface.encodeFunctionData('executionAddress', [
                    formatEvmV1Address(bundleOptions.executionAddress, this.interop2ChainId)
                ])
            );
        }
        if (bundleOptions.unbundlerAddress) {
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

        /// kl todo figure out what we need to wait for here. Probably the fact that we need to wait for the GW block finalization.
        // The line below does that, but it doesn't quite work for some reason.
        // await waitUntilBlockExecutedOnGateway(senderUtilityWallet, gatewayWallet, txReceipt!.blockNumber);
        await utils.sleep(25);
        const params = await senderUtilityWallet.getFinalizeWithdrawalParams(txHash, 0, 'proof_based_gw');
        await waitForInteropRootNonZero(this.interop2Provider, this.interop2RichWallet, getGWBlockNumber(params));
    }

    /**
     * Reads an interop transaction from the sender chain, constructs a new transaction,
     * and broadcasts it on the receiver chain.
     */
    async readAndBroadcastInteropBundle(txHash: string) {
        // Get interop trigger and bundle data from the sender chain.
        const executionBundle = await getInteropBundleData(this.interop1Provider, txHash, 0);
        if (executionBundle.output == null) return;

        const receipt = await this.interop2InteropHandler.executeBundle(
            executionBundle.rawData,
            executionBundle.proofDecoded
        );
        await receipt.wait();
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
     * Returns a random amount of ETH to transfer.
     */
    getTransferAmount(): bigint {
        return ethers.parseUnits((Math.floor(Math.random() * 900) + 100).toString(), 'gwei');
    }

    /**
     * Approves and mints a random amount of test tokens and returns the amount.
     */
    async getAndApproveTokenTransferAmount(): Promise<bigint> {
        const transferAmount = BigInt(Math.floor(Math.random() * 900) + 100);

        await Promise.all([
            // Approve token transfer on Interop1
            (await this.interop1TokenA.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, transferAmount)).wait(),
            // Mint tokens for the test wallet on Interop1 for the transfer
            (await this.interop1TokenA.mint(this.interop1Wallet.address, transferAmount)).wait()
        ]);

        return transferAmount;
    }

    async getAndApproveBridgedTokenTransferAmount(): Promise<bigint> {
        const transferAmount = BigInt(Math.floor(Math.random() * 900) + 100);

        await this.interop1RichWallet.transfer({
            to: this.interop1Wallet.address,
            amount: transferAmount,
            token: this.bridgedToken.l2Address
        });
        const bridgedTokenContract = new zksync.Contract(
            this.bridgedToken.l2Address,
            zksync.utils.IERC20,
            this.interop1Wallet
        );

        await Promise.all([
            // Approve token transfer on Interop1
            (await bridgedTokenContract.approve(L2_NATIVE_TOKEN_VAULT_ADDRESS, transferAmount)).wait()
        ]);

        return transferAmount;
    }
}
