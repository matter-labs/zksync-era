import * as zksync from 'zksync-web3';
import * as ethers from 'ethers';

import { TestContext, TestEnvironment, TestWallets } from './types';
import { lookupPrerequisites } from './prerequisites';
import { Reporter } from './reporter';
import { scaledGasPrice } from './helpers';
import { RetryProvider } from './retry-provider';

// These amounts of ETH would be provided to each test suite through its "main" account.
// It is assumed to be enough to run a set of "normal" transactions.
// If any test or test suite requires you to have more funds, please first think whether it can be avoided
// (e.g. use minted ERC20 token). If it's indeed necessary, deposit more funds from the "main" account separately.
//
// Please DO NOT change these constants if you don't know why you have to do that. Try to debug the particular issue
// you face first.
export const L1_DEFAULT_ETH_PER_ACCOUNT = ethers.utils.parseEther('0.08');
// Stress tests for L1->L2 transactions on localhost require a lot of upfront payment, but these are skipped during tests on normal environments
export const L1_EXTENDED_TESTS_ETH_PER_ACCOUNT = ethers.utils.parseEther('0.5');
export const L2_ETH_PER_ACCOUNT = ethers.utils.parseEther('0.5');
export const ERC20_PER_ACCOUNT = ethers.utils.parseEther('10000.0');

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

    private l1Provider: ethers.providers.JsonRpcProvider;
    private l2Provider: zksync.Provider;

    private reporter: Reporter = new Reporter();

    constructor(env: TestEnvironment) {
        this.env = env;

        this.reporter.message('Using L1 provider: ' + env.l1NodeUrl);
        this.reporter.message('Using L2 provider: ' + env.l2NodeUrl);

        this.l1Provider = new ethers.providers.JsonRpcProvider(env.l1NodeUrl);
        this.l2Provider = new RetryProvider(
            {
                url: env.l2NodeUrl,
                timeout: 1200 * 1000
            },
            undefined,
            this.reporter
        );

        if (env.network == 'localhost') {
            // Setup small polling interval on localhost to speed up tests.
            this.l1Provider.pollingInterval = 100;
            this.l2Provider.pollingInterval = 100;
        }

        this.mainEthersWallet = new ethers.Wallet(env.mainWalletPK, this.l1Provider);
        this.mainSyncWallet = new zksync.Wallet(env.mainWalletPK, this.l2Provider, this.l1Provider);
    }

    // Returns the required amount of L1 ETH
    requiredL1ETHPerAccount() {
        return this.env.network === 'localhost' ? L1_EXTENDED_TESTS_ETH_PER_ACCOUNT : L1_DEFAULT_ETH_PER_ACCOUNT;
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
        const latestNonce = await ethWallet.getTransactionCount('latest');
        const pendingNonce = await ethWallet.getTransactionCount('pending');
        this.reporter.debug(`Latest nonce is ${latestNonce}, pending nonce is ${pendingNonce}`);
        const cancellationTxs = [];
        for (let nonce = latestNonce; nonce < pendingNonce; nonce++) {
            // For each transaction to override it, we need to provide greater fee.
            // We would manually provide a value high enough (for a testnet) to be both valid
            // and higher than the previous one. It's OK as we'll only be charged for the bass fee
            // anyways. We will also set the miner's tip to 5 gwei, which is also much higher than the normal one.
            const maxFeePerGas = ethers.utils.parseEther('0.00000025'); // 250 gwei
            const maxPriorityFeePerGas = ethers.utils.parseEther('0.000000005'); // 5 gwei
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
        const accountsAmount = suites.length + 1;

        const l2ETHAmountToDeposit = await this.ensureBalances(accountsAmount);
        const l2ERC20AmountToDeposit = ERC20_PER_ACCOUNT.mul(accountsAmount);
        const wallets = this.createTestWallets(suites);
        await this.distributeL1Tokens(wallets, l2ETHAmountToDeposit, l2ERC20AmountToDeposit);
        await this.distributeL2Tokens(wallets);

        this.reporter.finishAction();
        return wallets;
    }

    /**
     * Checks the operator account balances on L1 and L2 and deposits funds if required.
     */
    private async ensureBalances(accountsAmount: number): Promise<ethers.BigNumber> {
        this.reporter.startAction(`Checking main account balance`);

        this.reporter.message(`Operator address is ${this.mainEthersWallet.address}`);

        const requiredL2ETHAmount = L2_ETH_PER_ACCOUNT.mul(accountsAmount);
        const actualL2ETHAmount = await this.mainSyncWallet.getBalance();
        this.reporter.message(`Operator balance on L2 is ${ethers.utils.formatEther(actualL2ETHAmount)} ETH`);

        // We may have enough funds in L2. If that's the case, no need to deposit more than required.
        const l2ETHAmountToDeposit = requiredL2ETHAmount.gt(actualL2ETHAmount)
            ? requiredL2ETHAmount.sub(actualL2ETHAmount)
            : ethers.BigNumber.from(0);

        const requiredL1ETHAmount = this.requiredL1ETHPerAccount().mul(accountsAmount).add(l2ETHAmountToDeposit);
        const actualL1ETHAmount = await this.mainSyncWallet.getBalanceL1();
        this.reporter.message(`Operator balance on L1 is ${ethers.utils.formatEther(actualL1ETHAmount)} ETH`);

        if (requiredL1ETHAmount.gt(actualL1ETHAmount)) {
            const required = ethers.utils.formatEther(requiredL1ETHAmount);
            const actual = ethers.utils.formatEther(actualL1ETHAmount);
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
    private async distributeL1Tokens(
        wallets: TestWallets,
        l2ETHAmountToDeposit: ethers.BigNumber,
        l2erc20DepositAmount: ethers.BigNumber
    ) {
        this.reporter.startAction(`Distributing tokens on L1`);
        const l1startNonce = await this.mainEthersWallet.getTransactionCount();
        this.reporter.debug(`Start nonce is ${l1startNonce}`);

        // All the promises we send in this function.
        const l1TxPromises: Promise<any>[] = [];
        // Mutable nonce to send the transactions before actually `await`ing them.
        let nonce = l1startNonce;
        // Scaled gas price to be used to prevent transactions from being stuck.
        const gasPrice = await scaledGasPrice(this.mainEthersWallet);

        // Deposit L2 tokens (if needed).
        if (!l2ETHAmountToDeposit.isZero()) {
            // Given that we've already sent a number of transactions,
            // we have to correctly send nonce.
            const depositHandle = this.mainSyncWallet
                .deposit({
                    token: zksync.utils.ETH_ADDRESS,
                    amount: l2ETHAmountToDeposit,
                    overrides: {
                        nonce: nonce++,
                        gasPrice
                    }
                })
                .then((tx) => {
                    const amount = ethers.utils.formatEther(l2ETHAmountToDeposit);
                    this.reporter.debug(`Sent ETH deposit. Nonce ${tx.nonce}, amount: ${amount}, hash: ${tx.hash}`);
                    tx.wait();
                });

            // Add this promise to the list of L1 tx promises.
            l1TxPromises.push(depositHandle);
        }

        // Define values for handling ERC20 transfers/deposits.
        const erc20Token = this.env.erc20Token.l1Address;
        const erc20MintAmount = l2erc20DepositAmount.mul(2);

        // Mint ERC20.
        const l1Erc20ABI = ['function mint(address to, uint256 amount)'];
        const l1Erc20Contract = new ethers.Contract(erc20Token, l1Erc20ABI, this.mainEthersWallet);
        const erc20MintPromise = l1Erc20Contract
            .mint(this.mainSyncWallet.address, erc20MintAmount, {
                nonce: nonce++,
                gasPrice
            })
            .then((tx: any) => {
                this.reporter.debug(`Sent ERC20 mint transaction. Hash: ${tx.hash}, nonce ${tx.nonce}`);
                return tx.wait();
            });

        // Deposit ERC20.
        const erc20DepositPromise = this.mainSyncWallet
            .deposit({
                token: erc20Token,
                amount: l2erc20DepositAmount,
                approveERC20: true,
                approveOverrides: {
                    nonce: nonce++,
                    gasPrice
                },
                overrides: {
                    nonce: nonce++,
                    gasPrice
                }
            })
            .then((tx) => {
                // Note: there is an `approve` tx, not listed here.
                this.reporter.debug(`Sent ERC20 deposit transaction. Hash: ${tx.hash}, nonce: ${tx.nonce}`);
                return tx.wait();
            });

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

        l1TxPromises.push(...ethTransfers);
        l1TxPromises.push(erc20MintPromise);
        l1TxPromises.push(erc20DepositPromise);
        l1TxPromises.push(...erc20Transfers);

        this.reporter.debug(`Sent ${l1TxPromises.length} initial transactions on L1`);

        await Promise.all(l1TxPromises);
        this.reporter.finishAction();
    }

    /**
     * Sends L2 tokens to the test wallet accounts.
     */
    private async distributeL2Tokens(wallets: TestWallets) {
        this.reporter.startAction(`Distributing tokens on L2`);
        let l2startNonce = await this.mainSyncWallet.getTransactionCount();

        // ETH transfers.
        const l2TxPromises = await sendTransfers(
            zksync.utils.ETH_ADDRESS,
            this.mainSyncWallet,
            wallets,
            L2_ETH_PER_ACCOUNT,
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
     * Performs context deinitialization.
     */
    async teardownContext() {
        // Reset the reporter context.
        this.reporter = new Reporter();
        try {
            this.reporter.startAction(`Tearing down the context`);

            await this.collectFunds();

            this.reporter.finishAction();
        } catch (error: any) {
            // Report the issue to the console and mark the last action as failed.
            this.reporter.error(`An error occurred: ${error.message || error}`);
            this.reporter.failAction();

            // Then propagate the exception.
            throw error;
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
    value: ethers.BigNumber,
    overrideStartNonce?: number,
    gasPrice?: ethers.BigNumber,
    reporter?: Reporter
): Promise<Promise<any>[]> {
    const erc20Contract =
        wallet instanceof zksync.Wallet
            ? new zksync.Contract(token, zksync.utils.IERC20, wallet)
            : new ethers.Contract(token, zksync.utils.IERC20, wallet);
    const startNonce = overrideStartNonce ?? (await wallet.getTransactionCount());
    reporter?.debug(`Sending transfers. Token address is ${token}`);
    const txPromises = Array.from(Object.values(wallets)).map((testWalletPK, index) => {
        if (token == zksync.utils.ETH_ADDRESS) {
            const tx = {
                to: ethers.utils.computeAddress(testWalletPK),
                value,
                nonce: startNonce + index,
                gasPrice
            };

            reporter?.debug(`Inititated ETH transfer with nonce: ${tx.nonce}`);
            return wallet.sendTransaction(tx).then((tx) => {
                reporter?.debug(`Sent ETH transfer tx: ${tx.hash}, nonce: ${tx.nonce}`);
                return tx.wait();
            });
        } else {
            const txNonce = startNonce + index;
            const tx = erc20Contract.transfer(ethers.utils.computeAddress(testWalletPK), value, {
                nonce: txNonce,
                gasPrice
            });
            reporter?.debug(`Inititated ERC20 transfer with nonce: ${txNonce}`);
            // @ts-ignore
            return tx.then((tx) => {
                reporter?.debug(`Sent ERC20 transfer tx: ${tx.hash}, nonce: ${tx.nonce}`);
                return tx.wait();
            });
        }
    });
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
        const transferPrice = gasLimit.mul(gasPrice);

        const balance = await from.getBalance();

        // If we can't afford sending funds back (or the wallet is empty), do nothing.
        if (transferPrice.gt(balance)) {
            continue;
        }

        const value = balance.sub(transferPrice);

        reporter?.debug(
            `Wallet balance: ${ethers.utils.formatEther(balance)} ETH,\
             estimated cost is ${ethers.utils.formatEther(transferPrice)} ETH,\
             value for tx is ${ethers.utils.formatEther(value)} ETH`
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
type ReceiptFuture = Promise<ethers.ethers.providers.TransactionReceipt>;
