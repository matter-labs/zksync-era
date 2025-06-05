import * as zksync from 'zksync-ethers';
import * as ethers from 'ethers';
import { BigNumberish } from 'ethers';

import { NodeMode, TestContext, TestEnvironment, TestWallets } from './types';
import { lookupPrerequisites } from './prerequisites';
import { Reporter } from './reporter';
import { isLocalHost, scaledGasPrice } from './helpers';
import { RetryProvider } from './retry-provider';
import { killPidWithAllChilds } from 'utils/build/kill';

// These amounts of ETH would be provided to each test suite through its "main" account.
// It is assumed to be enough to run a set of "normal" transactions.
// If any test or test suite requires you to have more funds, please first think whether it can be avoided
// (e.g. use minted ERC20 token). If it's indeed necessary, deposit more funds from the "main" account separately.
//
// Please DO NOT change these constants if you don't know why you have to do that. Try to debug the particular issue
// you face first.
export const L1_DEFAULT_ETH_PER_ACCOUNT = ethers.parseEther('0.08');
// Stress tests for L1->L2 transactions on localhost require a lot of upfront payment, but these are skipped during tests on normal environments
export const L1_EXTENDED_TESTS_ETH_PER_ACCOUNT = ethers.parseEther('0.5');
export const L2_DEFAULT_ETH_PER_ACCOUNT = ethers.parseEther('0.5');

// Stress tests on local host may require a lot of additiomal funds, but these are skipped during tests on normal environments
export const L2_EXTENDED_TESTS_ETH_PER_ACCOUNT = ethers.parseEther('50');
export const ERC20_PER_ACCOUNT = ethers.parseEther('10000.0');

interface VmPlaygroundHealth {
    readonly status: string;
    readonly details?: {
        vm_mode?: string;
        last_processed_batch?: number;
    };
}

interface TxSenderHealth {
    readonly status: string;
    readonly details?: {
        vm_mode?: string;
        vm_divergences?: number;
    };
}

interface NodeHealth {
    readonly components: {
        vm_playground?: VmPlaygroundHealth;
        tx_sender?: TxSenderHealth;
    };
}

/**
 * This class is responsible for preparing the test environment for all the other test suites.
 *
 * ## Context initialization
 *
 * To enable test suites to do their checks, this class:
 * - Waits for server to launch (required for running tests on stage right after deployment).
 * - Ensures that "master" wallet has enough balances to perform the tests.
 * - Prepares wallets for each test suite and adds funds to them.
 * - Deploys the test contracts.
 *
 * ## Performed checks
 *
 * Given the fact that all the test suites would be run in parallel, while context initialization
 * is performed sequentially, during initialization this class also performs several basic "sanity"
 * checks (e.g. "ether transfers works" and "fees are being collected").
 * Checks performed by this class should belong to one of the following categories:
 * 1) It's a "building block" for other tests (e.g., if contract deployments don't work, other test suites won't work as well).
 * 2) It must be run sequentially (e.g. it's hard to ensure that fee account received exact amount of fee if multiple processes
 *    send transactions).
 *
 * Important!
 * Only add the essential checks to this class, as it should be kept minimal. Whenever possible, prefer creating a new test suite
 * or extending an existing one.
 */
export class TestContextOwner {
    private env: TestEnvironment;
    private wallets?: TestWallets;

    private mainEthersWallet: ethers.Wallet;
    private mainSyncWallet: zksync.Wallet;

    private l1Provider: ethers.JsonRpcProvider;
    private l2Provider: zksync.Provider;

    private reporter: Reporter = new Reporter();

    constructor(env: TestEnvironment) {
        this.env = env;

        this.reporter.message('Using L1 provider: ' + env.l1NodeUrl);
        this.reporter.message('Using L2 provider: ' + env.l2NodeUrl);

        this.l1Provider = new ethers.JsonRpcProvider(env.l1NodeUrl);
        this.l2Provider = new RetryProvider(
            {
                url: env.l2NodeUrl,
                timeout: 1200 * 1000
            },
            undefined,
            this.reporter
        );

        if (isLocalHost(env.network)) {
            // Setup small polling interval on localhost to speed up tests.
            this.l1Provider.pollingInterval = 100;
            this.l2Provider.pollingInterval = 100;
        }

        this.mainEthersWallet = new ethers.Wallet(env.mainWalletPK, this.l1Provider);
        this.mainSyncWallet = new zksync.Wallet(env.mainWalletPK, this.l2Provider, this.l1Provider);
    }

    // Returns the required amount of L1 ETH
    requiredL1ETHPerAccount() {
        return isLocalHost(this.env.network) ? L1_EXTENDED_TESTS_ETH_PER_ACCOUNT : L1_DEFAULT_ETH_PER_ACCOUNT;
    }

    // Returns the required amount of L2 ETH
    requiredL2ETHPerAccount() {
        return isLocalHost(this.env.network) ? L2_EXTENDED_TESTS_ETH_PER_ACCOUNT : L2_DEFAULT_ETH_PER_ACCOUNT;
    }

    /**
     * Performs the test context initialization.
     *
     * @returns Context object required for test suites.
     */
    async setupContext(): Promise<TestContext> {
        try {
            this.reporter.startAction('Setting up the context');
            await this.cancelPendingTxs();
            await this.cancelAllowances();
            this.wallets = await this.prepareWallets();
            this.reporter.finishAction();
        } catch (error: any) {
            // Report the issue to the console and mark the last action as failed.
            this.reporter.error(`An error occurred: ${error.message || error}`);
            this.reporter.failAction();

            // Then propagate the exception.
            throw error;
        }
        return {
            wallets: this.wallets,
            environment: this.env
        };
    }

    /**
     * Checks if there are any pending transactions initiated from the main wallet.
     * If such transactions are found, cancels them by sending blank ones with exaggregated fee allowance.
     */
    private async cancelPendingTxs() {
        this.reporter.startAction(`Cancelling pending transactions`);
        // Since some tx may be pending on stage, we don't want to get stuck because of it.
        // In order to not get stuck transactions, we manually cancel all the pending txs.
        const ethWallet = this.mainEthersWallet;
        const latestNonce = await ethWallet.getNonce('latest');
        const pendingNonce = await ethWallet.getNonce('pending');
        this.reporter.debug(`Latest nonce is ${latestNonce}, pending nonce is ${pendingNonce}`);
        // For each transaction to override it, we need to provide greater fee.
        // We would manually provide a value high enough (for a testnet) to be both valid
        // and higher than the previous one. It's OK as we'll only be charged for the base fee
        // anyways. We will also set the miner's tip to 5 gwei, which is also much higher than the normal one.
        // Scaled gas price to be used to prevent transactions from being stuck.
        const maxPriorityFeePerGas = ethers.parseEther('0.000000005'); // 5 gwei
        const maxFeePerGas = ethers.parseEther('0.00000025'); // 250 gwei
        this.reporter.debug(`Max nonce is ${latestNonce}, pending nonce is ${pendingNonce}`);

        const cancellationTxs = [];
        for (let nonce = latestNonce; nonce < pendingNonce; nonce++) {
            cancellationTxs.push(
                ethWallet
                    .sendTransaction({ to: ethWallet.address, nonce, maxFeePerGas, maxPriorityFeePerGas })
                    .then((tx) => tx.wait())
            );
        }
        if (cancellationTxs.length > 0) {
            await Promise.all(cancellationTxs);
            this.reporter.message(`Canceled ${cancellationTxs.length} pending transactions`);
        }
        this.reporter.finishAction();
    }

    /**
     * Sets allowances to 0 for tokens. We do this so we can predict nonces accurately.
     */
    private async cancelAllowances() {
        this.reporter.startAction(`Cancelling allowances transactions`);
        // Since some tx may be pending on stage, we don't want to get stuck because of it.
        // In order to not get stuck transactions, we manually cancel all the pending txs.
        const chainId = this.env.l2ChainId;

        const bridgehub = await this.mainSyncWallet.getBridgehubContract();
        const erc20Bridge = await this.mainSyncWallet.getL1AssetRouter();
        const baseToken = await bridgehub.baseToken(chainId);

        const erc20Token = this.env.erc20Token.l1Address;

        const l1Erc20ABI = ['function approve(address spender, uint256 amount)'];
        const l1Erc20Contract = new ethers.Contract(erc20Token, l1Erc20ABI, this.mainEthersWallet);
        const tx = await l1Erc20Contract.approve(erc20Bridge, 0);
        await tx.wait();
        this.reporter.debug(`Sent ERC20 cancel approve transaction. Hash: ${tx.hash}, nonce ${tx.nonce}`);

        if (baseToken != zksync.utils.ETH_ADDRESS_IN_CONTRACTS) {
            const baseTokenContract = new ethers.Contract(baseToken, l1Erc20ABI, this.mainEthersWallet);
            const tx = await baseTokenContract.approve(erc20Bridge, 0);
            await tx.wait();
            this.reporter.debug(`Sent base token cancel approve transaction. Hash: ${tx.hash}, nonce ${tx.nonce}`);
        }

        this.reporter.finishAction();
    }

    /**
     * Looks for the declared test suites, prepares wallets for each test suite
     * and adds funds to them.
     *
     * @returns Object containing private keys of wallets for each test suite.
     */
    private async prepareWallets(): Promise<TestWallets> {
        this.reporter.startAction(`Preparing wallets`);
        const suites = lookupPrerequisites();
        this.reporter.message(`Found following suites: ${suites.join(', ')}`);

        // `+ 1  for the main account (it has to send all these transactions).
        const accountsAmount = BigInt(suites.length) + 1n;

        const l2ETHAmountToDeposit = await this.ensureBalances(accountsAmount);
        const l2ERC20AmountToDeposit = ERC20_PER_ACCOUNT * accountsAmount;
        const wallets = this.createTestWallets(suites);
        const bridgehubContract = await this.mainSyncWallet.getBridgehubContract();
        const baseTokenAddress = await bridgehubContract.baseToken(this.env.l2ChainId);
        await this.distributeL1BaseToken(wallets, l2ERC20AmountToDeposit, baseTokenAddress);
        await this.cancelAllowances();
        await this.distributeL1Tokens(wallets, l2ETHAmountToDeposit, l2ERC20AmountToDeposit, baseTokenAddress);
        await this.distributeL2Tokens(wallets);

        this.reporter.finishAction();
        return wallets;
    }

    /**
     * Checks the operator account balances on L1 and L2 and deposits funds if required.
     */
    private async ensureBalances(accountsAmount: bigint): Promise<bigint> {
        this.reporter.startAction(`Checking main account balance`);

        this.reporter.message(`Operator address is ${this.mainEthersWallet.address}`);

        const requiredL2ETHAmount = this.requiredL2ETHPerAccount() * accountsAmount;
        const actualL2ETHAmount = await this.mainSyncWallet.getBalance();
        this.reporter.message(`Operator balance on L2 is ${ethers.formatEther(actualL2ETHAmount)} ETH`);

        // We may have enough funds in L2. If that's the case, no need to deposit more than required.
        const l2ETHAmountToDeposit =
            requiredL2ETHAmount > actualL2ETHAmount ? requiredL2ETHAmount - actualL2ETHAmount : 0n;

        const requiredL1ETHAmount = this.requiredL1ETHPerAccount() * accountsAmount + l2ETHAmountToDeposit;
        const actualL1ETHAmount = await this.mainSyncWallet.getBalanceL1();
        this.reporter.message(`Operator balance on L1 is ${ethers.formatEther(actualL1ETHAmount)} ETH`);

        if (requiredL1ETHAmount > actualL1ETHAmount) {
            const required = ethers.formatEther(requiredL1ETHAmount);
            const actual = ethers.formatEther(actualL1ETHAmount);
            const errorMessage = `There must be at least ${required} ETH on main account, but only ${actual} is available`;
            throw new Error(errorMessage);
        }
        this.reporter.finishAction();

        return l2ETHAmountToDeposit;
    }

    /**
     * Generates wallet objects for the test suites.
     */
    private createTestWallets(suites: string[]): TestWallets {
        this.reporter.startAction(`Creating test wallets`);
        const wallets: TestWallets = {};
        for (const suiteFile of suites) {
            const randomWallet = ethers.Wallet.createRandom().privateKey;
            wallets[suiteFile] = randomWallet;
        }
        this.reporter.debug(`Test wallets: ${JSON.stringify(wallets, undefined, 2)}`);
        this.reporter.finishAction();
        return wallets;
    }

    /**
     * Sends L1 tokens to the test wallet accounts.
     * Additionally, deposits L1 tokens to the main account for further distribution on L2 (if required).
     */
    private async distributeL1BaseToken(
        wallets: TestWallets,
        l2erc20DepositAmount: bigint,
        baseTokenAddress: zksync.types.Address
    ) {
        this.reporter.debug(`Base token address is ${baseTokenAddress}`);
        const ethIsBaseToken = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;
        this.reporter.startAction(`Distributing base tokens on L1`);
        if (!ethIsBaseToken) {
            const l1startNonce = await this.mainEthersWallet.getNonce();
            this.reporter.debug(`Start nonce is ${l1startNonce}`);
            // All the promises we send in this function.
            const l1TxPromises: Promise<any>[] = [];
            // Mutable nonce to send the transactions before actually `await`ing them.
            let nonce = l1startNonce;
            // Scaled gas price to be used to prevent transactions from being stuck.
            const gasPrice = await scaledGasPrice(this.mainEthersWallet);

            // Define values for handling ERC20 transfers/deposits.
            const baseMintAmount = l2erc20DepositAmount * 1000n;
            // Mint ERC20.
            const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
            const l1Erc20Contract = new ethers.Contract(baseTokenAddress, l1Erc20ABI, this.mainEthersWallet);
            const baseMintPromise = l1Erc20Contract
                .mint(this.mainSyncWallet.address, baseMintAmount, {
                    nonce: nonce++,
                    gasPrice
                })
                .then((tx: any) => {
                    this.reporter.debug(`Sent ERC20 mint transaction. Hash: ${tx.hash}, tx nonce ${tx.nonce}`);
                    return tx.wait();
                });
            this.reporter.debug(`Nonce changed by 1 for ERC20 mint, new nonce: ${nonce}`);
            await baseMintPromise;

            // Deposit base token if needed
            const baseIsTransferred = true;
            const baseDepositPromise = this.mainSyncWallet
                .deposit({
                    token: baseTokenAddress,
                    amount: l2erc20DepositAmount,
                    approveERC20: true,
                    approveBaseERC20: true,
                    approveBaseOverrides: {
                        nonce: nonce,
                        gasPrice
                    },
                    approveOverrides: {
                        nonce: nonce + (ethIsBaseToken ? 0 : 1), // if eth is base, we don't need to approve base
                        gasPrice
                    },
                    overrides: {
                        nonce: nonce + (ethIsBaseToken ? 0 : 1) + (baseIsTransferred ? 0 : 1), // if base is transferred, we don't need to approve override
                        gasPrice
                    }
                })
                .then((tx) => {
                    // Note: there is an `approve` tx, not listed here.
                    this.reporter.debug(`Sent ERC20 deposit transaction. Hash: ${tx.hash}, tx nonce: ${tx.nonce}`);
                    return tx.wait();
                });
            nonce = nonce + 1 + (ethIsBaseToken ? 0 : 1) + (baseIsTransferred ? 0 : 1);
            this.reporter.debug(
                `Nonce changed by ${
                    1 + (ethIsBaseToken ? 0 : 1) + (baseIsTransferred ? 0 : 1)
                } for ERC20 deposit, new nonce: ${nonce}`
            );
            // Send base token on L1.
            const baseTokenTransfers = await sendTransfers(
                baseTokenAddress,
                this.mainEthersWallet,
                wallets,
                ERC20_PER_ACCOUNT,
                nonce,
                gasPrice,
                this.reporter
            );

            l1TxPromises.push(baseDepositPromise);
            l1TxPromises.push(...baseTokenTransfers);

            this.reporter.debug(`Sent ${l1TxPromises.length} base token initial transactions on L1`);
            await Promise.all(l1TxPromises);
        }
        this.reporter.finishAction();
    }

    /**
     * Sends L1 tokens to the test wallet accounts.
     * Additionally, deposits L1 tokens to the main account for further distribution on L2 (if required).
     */
    private async distributeL1Tokens(
        wallets: TestWallets,
        l2ETHAmountToDeposit: bigint,
        l2erc20DepositAmount: bigint,
        baseTokenAddress: zksync.types.Address
    ) {
        const ethIsBaseToken = baseTokenAddress == zksync.utils.ETH_ADDRESS_IN_CONTRACTS;
        this.reporter.startAction(`Distributing tokens on L1`);
        const l1startNonce = await this.mainEthersWallet.getNonce();
        this.reporter.debug(`Start nonce is ${l1startNonce}`);
        // All the promises we send in this function.
        const l1TxPromises: Promise<any>[] = [];
        // Mutable nonce to send the transactions before actually `await`ing them.
        let nonce = l1startNonce;
        // Scaled gas price to be used to prevent transactions from being stuck.
        const gasPrice = await scaledGasPrice(this.mainEthersWallet);

        // Deposit L2 tokens (if needed).
        if (l2ETHAmountToDeposit != 0n) {
            // Given that we've already sent a number of transactions,
            // we have to correctly send nonce.
            const depositHandle = this.mainSyncWallet
                .deposit({
                    token: zksync.utils.ETH_ADDRESS,
                    approveBaseERC20: true,
                    approveERC20: true,
                    amount: l2ETHAmountToDeposit as BigNumberish,
                    approveBaseOverrides: {
                        nonce: nonce,
                        gasPrice
                    },
                    overrides: {
                        nonce: nonce + (ethIsBaseToken ? 0 : 1), // if eth is base token the approve tx does not happen
                        gasPrice
                    }
                })
                .then((tx) => {
                    const amount = ethers.formatEther(l2ETHAmountToDeposit);
                    this.reporter.debug(`Sent ETH deposit. Nonce ${tx.nonce}, amount: ${amount}, hash: ${tx.hash}`);
                    return tx.wait();
                });
            nonce = nonce + 1 + (ethIsBaseToken ? 0 : 1);
            this.reporter.debug(
                `Nonce changed by ${1 + (ethIsBaseToken ? 0 : 1)} for ETH deposit, new nonce: ${nonce}`
            );
            await depositHandle;
        }
        // Define values for handling ERC20 transfers/deposits.
        const erc20Token = this.env.erc20Token.l1Address;
        const erc20MintAmount = l2erc20DepositAmount * 100n;
        // Mint ERC20.
        const baseIsTransferred = false; // we are not transferring the base
        const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
        const l1Erc20Contract = new ethers.Contract(erc20Token, l1Erc20ABI, this.mainEthersWallet);
        const gasLimit = await l1Erc20Contract.mint.estimateGas(this.mainSyncWallet.address, erc20MintAmount);
        const erc20MintPromise = l1Erc20Contract
            .mint(this.mainSyncWallet.address, erc20MintAmount, {
                nonce: nonce++,
                gasPrice,
                gasLimit
            })
            .then((tx: any) => {
                this.reporter.debug(`Sent ERC20 mint transaction. Hash: ${tx.hash}, nonce ${tx.nonce}`);
                return tx.wait();
            });
        this.reporter.debug(`Nonce changed by 1 for ERC20 mint, new nonce: ${nonce}`);
        await erc20MintPromise;
        // Deposit ERC20.
        const erc20DepositPromise = this.mainSyncWallet
            .deposit({
                token: erc20Token,
                amount: l2erc20DepositAmount,
                approveERC20: true,
                approveBaseERC20: true,
                approveBaseOverrides: {
                    nonce: nonce,
                    gasPrice
                },
                approveOverrides: {
                    nonce: nonce + (ethIsBaseToken ? 0 : 1), // if eth is base, we don't need to approve base
                    gasPrice
                },
                overrides: {
                    nonce: nonce + (ethIsBaseToken ? 0 : 1) + (baseIsTransferred ? 0 : 1), // if base is transferred, we don't need to approve override
                    gasPrice
                }
            })
            .then((tx) => {
                // Note: there is an `approve` tx, not listed here.
                this.reporter.debug(`Sent ERC20 deposit transaction. Hash: ${tx.hash}, nonce: ${tx.nonce}`);
                return tx.wait();
            });
        nonce = nonce + 1 + (ethIsBaseToken ? 0 : 1) + (baseIsTransferred ? 0 : 1);
        this.reporter.debug(
            `Nonce changed by ${
                1 + (ethIsBaseToken ? 0 : 1) + (baseIsTransferred ? 0 : 1)
            } for ERC20 deposit, new nonce: ${nonce}`
        );
        // Send ETH on L1.
        const ethTransfers = await sendTransfers(
            zksync.utils.ETH_ADDRESS,
            this.mainEthersWallet,
            wallets,
            this.requiredL1ETHPerAccount(),
            nonce,
            gasPrice,
            this.reporter
        );
        nonce += ethTransfers.length;

        // Send ERC20 on L1.
        const erc20Transfers = await sendTransfers(
            erc20Token,
            this.mainEthersWallet,
            wallets,
            ERC20_PER_ACCOUNT,
            nonce,
            gasPrice,
            this.reporter
        );

        nonce += erc20Transfers.length;
        // Send ERC20 base token on L1.
        const baseErc20Transfers = await sendTransfers(
            baseTokenAddress,
            this.mainEthersWallet,
            wallets,
            ERC20_PER_ACCOUNT,
            nonce,
            gasPrice,
            this.reporter
        );

        l1TxPromises.push(erc20DepositPromise);
        l1TxPromises.push(...ethTransfers);
        l1TxPromises.push(...erc20Transfers);
        l1TxPromises.push(...baseErc20Transfers);

        this.reporter.debug(`Sent ${l1TxPromises.length} initial transactions on L1`);
        await Promise.all(l1TxPromises);

        this.reporter.finishAction();
    }

    /**
     * Sends L2 tokens to the test wallet accounts.
     */
    private async distributeL2Tokens(wallets: TestWallets) {
        this.reporter.startAction(`Distributing tokens on L2`);
        let l2startNonce = await this.mainSyncWallet.getNonce();

        // ETH transfers.
        const l2TxPromises = await sendTransfers(
            zksync.utils.ETH_ADDRESS,
            this.mainSyncWallet,
            wallets,
            this.requiredL2ETHPerAccount(),
            l2startNonce,
            undefined,
            this.reporter
        );
        l2startNonce += l2TxPromises.length;

        // ERC20 transfers.
        const l2TokenAddress = await this.mainSyncWallet.l2TokenAddress(this.env.erc20Token.l1Address);
        const erc20Promises = await sendTransfers(
            l2TokenAddress,
            this.mainSyncWallet,
            wallets,
            ERC20_PER_ACCOUNT,
            l2startNonce,
            undefined,
            this.reporter
        );
        l2TxPromises.push(...erc20Promises);
        await Promise.all(l2TxPromises);

        this.reporter.finishAction();
    }

    /**
     * Waits until the VM playground processes all L1 batches. If the playground runs the new VM in the shadow mode, this means
     * that there are no divergence in old and new VM execution. Outputs a warning if the VM playground isn't run or runs not in the shadow mode.
     */
    private async waitForVmPlayground() {
        while (true) {
            const lastProcessedBatch = await this.lastPlaygroundBatch();
            if (lastProcessedBatch === undefined) {
                this.reporter.warn('The node does not run VM playground; run to check old / new VM divergence');
                break;
            }
            const lastNodeBatch = await this.l2Provider.getL1BatchNumber();
            this.reporter.debug(`VM playground progress: L1 batch #${lastProcessedBatch} / ${lastNodeBatch}`);
            if (lastProcessedBatch >= lastNodeBatch) {
                break;
            }
            await zksync.utils.sleep(500);
        }
    }

    /**
     * Returns the number of the last L1 batch processed by the VM playground, taking it from the node health endpoint.
     * Returns `undefined` if the VM playground isn't run or doesn't have the shadow mode.
     */
    private async lastPlaygroundBatch() {
        const healthcheckPort = this.env.healthcheckPort;
        const nodeHealth = (await (await fetch(`http://127.0.0.1:${healthcheckPort}/health`)).json()) as NodeHealth;
        const playgroundHealth = nodeHealth.components.vm_playground;
        if (playgroundHealth === undefined) {
            return undefined;
        }
        if (playgroundHealth.status !== 'ready') {
            throw new Error(`Unexpected VM playground health status: ${playgroundHealth.status}`);
        }
        if (playgroundHealth.details?.vm_mode !== 'shadow') {
            this.reporter.warn(
                `VM playground mode is '${playgroundHealth.details?.vm_mode}'; should be set to 'shadow' to check VM divergence`
            );
            return undefined;
        }
        return playgroundHealth.details?.last_processed_batch ?? 0;
    }

    private async checkVmDivergencesInApi() {
        const healthcheckPort = this.env.healthcheckPort;
        const nodeHealth = (await (await fetch(`http://127.0.0.1:${healthcheckPort}/health`)).json()) as NodeHealth;
        const txSenderHealth = nodeHealth.components.tx_sender;
        if (txSenderHealth === undefined) {
            throw new Error('No tx_sender in node components');
        }
        const vmMode = txSenderHealth.details?.vm_mode;
        if (vmMode !== 'shadow') {
            this.reporter.warn(`API VM mode is '${vmMode}'; should be set to 'shadow' to check VM divergence`);
            return;
        }
        const divergenceCount = txSenderHealth.details?.vm_divergences ?? 0;
        if (divergenceCount > 0) {
            throw new Error(`${divergenceCount} VM divergence(s) were detected!`);
        }
    }

    /**
     * Performs context deinitialization.
     */
    async teardownContext() {
        // Reset the reporter context.
        this.reporter = new Reporter();
        try {
            if (isLocalHost(this.env.network)) {
                this.reporter.startAction('Checking for VM divergences in API');
                await this.checkVmDivergencesInApi();
                this.reporter.finishAction();
            }

            if (this.env.nodeMode == NodeMode.Main && isLocalHost(this.env.network)) {
                // Check that the VM execution hasn't diverged using the VM playground. The component and thus the main node
                // will crash on divergence, so we just need to make sure that the test doesn't exit before the VM playground
                // processes all batches on the node.
                this.reporter.startAction('Waiting for VM playground to catch up');
                await this.waitForVmPlayground();
                this.reporter.finishAction();
            }
            this.reporter.startAction(`Collecting funds`);
            await this.collectFunds();
            this.reporter.finishAction();
            this.reporter.startAction(`Destroying providers`);
            // Destroy providers so that they drop potentially active connections to the node. Not doing so might cause
            // unexpected network errors to propagate during node termination.
            try {
                this.l1Provider.destroy();
            } catch (err: any) {
                // Catch any request cancellation errors that propagate here after destroying L1 provider
                console.log(`Caught error while destroying L1 provider: ${err}`);
            }
            try {
                this.l2Provider.destroy();
            } catch (err: any) {
                // Catch any request cancellation errors that propagate here after destroying L2 provider
                console.log(`Caught error while destroying L2 provider: ${err}`);
            }
            this.reporter.finishAction();
        } catch (error: any) {
            // Report the issue to the console and mark the last action as failed.
            this.reporter.error(`An error occurred: ${error.message || error}`);
            this.reporter.failAction();

            // Then propagate the exception.
            throw error;
        }
        if (this.env.l2NodePid !== undefined) {
            this.reporter.startAction(`Terminating L2 node process`);
            await killPidWithAllChilds(this.env.l2NodePid, 9);
            this.reporter.finishAction();
        }
    }

    /**
     * Returns funds from suite-specific wallets back to the main account.
     */
    private async collectFunds() {
        this.reporter.startAction(`Collecting funds back to the main account`);

        const l1Wallets = Object.values(this.wallets!).map((pk) => new ethers.Wallet(pk, this.l1Provider));
        const l2Wallets = Object.values(this.wallets!).map(
            (pk) => new zksync.Wallet(pk, this.l2Provider, this.l1Provider)
        );
        const wallets = l1Wallets.concat(l2Wallets);

        const txPromises: ReceiptFuture[] = await claimEtherBack(wallets, this.mainEthersWallet.address, this.reporter);

        await Promise.all(txPromises);

        this.reporter.finishAction();

        // We don't really need to withdraw funds back, since test takes existing L2 balance
        // into account. If the same wallet would be reused (e.g. on stage), it'll just have to
        // deposit less next time.
    }

    setL2NodePid(newPid: number) {
        this.env.l2NodePid = newPid;
    }
}

/**
 * Sends transfer from the "main" wallet to the list of "receiver" wallets.
 * Can work both with L1 and L2 wallets.
 *
 * @param wallet Main wallet to send Ether from
 * @param wallets Receiver wallets.
 * @param value Amount of Ether to distribute.
 * @param overrideStartNonce (optional): Nonce to use for the first transaction.
 * @param gasPrice (optional): Gas price to use in transactions.
 * @param reporter (optional): Reporter object to write logs to.
 * @returns List of promises for each sent transaction.
 */
export async function sendTransfers(
    token: string,
    wallet: ethers.Wallet | zksync.Wallet,
    wallets: TestWallets,
    value: bigint,
    overrideStartNonce?: number,
    gasPrice?: bigint,
    reporter?: Reporter
): Promise<Promise<any>[]> {
    const erc20Contract =
        wallet instanceof zksync.Wallet
            ? new zksync.Contract(token, zksync.utils.IERC20, wallet)
            : new ethers.Contract(token, zksync.utils.IERC20, wallet);
    const startNonce = overrideStartNonce ?? (await wallet.getNonce());
    reporter?.debug(`Sending transfers. Token address is ${token}`);

    const walletsPK = Array.from(Object.values(wallets));

    const txPromises: Promise<any>[] = [];

    for (let index = 0; index < walletsPK.length; index++) {
        const testWalletPK = walletsPK[index];
        if (token == zksync.utils.ETH_ADDRESS) {
            const tx = {
                to: ethers.computeAddress(testWalletPK),
                value,
                nonce: startNonce + index,
                gasPrice
            };

            reporter?.debug(`Inititated ETH transfer with nonce: ${tx.nonce}`);
            let transactionResponse = await wallet.sendTransaction(tx);
            reporter?.debug(`Sent ETH transfer tx: ${transactionResponse.hash}, nonce: ${transactionResponse.nonce}`);

            txPromises.push(
                transactionResponse.wait().then((tx) => {
                    reporter?.debug(`Obtained receipt for ETH transfer tx: ${tx?.hash} `);
                    return tx;
                })
            );
        } else {
            const txNonce = startNonce + index;
            reporter?.debug(`Inititated ERC20 transfer with nonce: ${txNonce}`);
            const gasLimit = await erc20Contract.transfer.estimateGas(ethers.computeAddress(testWalletPK), value);
            const tx = await erc20Contract.transfer(ethers.computeAddress(testWalletPK), value, {
                nonce: txNonce,
                gasPrice,
                gasLimit
            });
            reporter?.debug(`Sent ERC20 transfer tx: ${tx.hash}, nonce: ${tx.nonce}`);

            txPromises.push(
                // @ts-ignore
                tx.wait().then((tx) => {
                    reporter?.debug(`Obtained receipt for ERC20 transfer tx: ${tx.hash}`);
                    return tx;
                })
            );
        }
    }

    reporter?.debug(
        `Initiated ${txPromises.length} transfers. Nonce range is ${startNonce} - ${startNonce + txPromises.length - 1}`
    );

    return txPromises;
}

/**
 * Sends all the Ether from one account to another.
 * Can work both with L1 and L2 wallets.
 *
 * @param from Initiator wallet
 * @param toAddress Address of the receiver wallet.
 * @returns Promise for the transaction.
 */
export async function claimEtherBack(
    wallets: ethers.Wallet[] | zksync.Wallet[],
    toAddress: string,
    reporter?: Reporter
): Promise<Promise<any>[]> {
    const promises = [];

    for (const from of wallets) {
        // We do this for each wallets separately, since we may have L1/L2 objects together in the list.
        let gasLimit;
        try {
            gasLimit = await from.estimateGas({ value: 1, to: toAddress });
        } catch (_error) {
            // If gas estimation fails, we just skip this wallet.
            continue;
        }
        // We use scaled gas price to increase chances of tx not being stuck.
        const gasPrice = await scaledGasPrice(from);
        const transferPrice = gasLimit * gasPrice;

        const balance = await from.provider!.getBalance(from.address);

        // If we can't afford sending funds back (or the wallet is empty), do nothing.
        if (transferPrice > balance) {
            continue;
        }

        const value = balance - transferPrice;

        reporter?.debug(
            `Wallet balance: ${ethers.formatEther(balance)} ETH,\
             estimated cost is ${ethers.formatEther(transferPrice)} ETH,\
             value for tx is ${ethers.formatEther(value)} ETH`
        );

        const txPromise = from
            .sendTransaction({
                to: toAddress,
                value,
                gasLimit,
                gasPrice
            })
            .then((tx) => {
                reporter?.debug(`Sent tx: ${tx.hash}`);
                return tx.wait();
            })
            .catch((reason) => {
                // We don't care about failed transactions.
                reporter?.debug(`One of the transactions failed. Info: ${reason}`);
            });

        promises.push(txPromise);
    }

    return promises;
}

/**
 * Type represents a transaction that may have been sent.
 */
type ReceiptFuture = Promise<ethers.TransactionReceipt>;
