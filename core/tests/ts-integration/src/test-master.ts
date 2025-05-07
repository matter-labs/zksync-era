// import * as zksync from 'zksync-ethers';
import * as zksync from 'zksync-ethers-interop-support';
import * as ethers from 'ethers';
import { TestEnvironment, TestContext } from './types';
import { claimEtherBack } from './context-owner';
import { EthersRetryProvider, RetryableWallet, RetryProvider } from './retry-provider';
import { Reporter } from './reporter';
import { bigIntReviver, isLocalHost } from './helpers';

/**
 * Test master is a singleton class (per suite) that is capable of providing wallets to the suite.
 *
 * It loads one funded wallet from the initialized context, and can create new empty wallets.
 * This class keeps track of all the wallets that were created and after tests it collects funds back.
 *
 * Additionally, it also provides access to the test environment
 */
export class TestMaster {
    private static _instance?: TestMaster;

    private readonly env: TestEnvironment;
    readonly reporter: Reporter;
    private readonly l1Provider: EthersRetryProvider;
    private readonly l2Provider: RetryProvider;

    private readonly mainWallet: RetryableWallet;
    private readonly subAccounts: zksync.Wallet[] = [];

    private constructor(file: string) {
        if (TestMaster._instance) {
            throw new Error('Use TestMaster.getInstance instead of constructor');
        }

        const contextStr = process.env.ZKSYNC_JEST_TEST_CONTEXT;
        if (!contextStr) {
            throw new Error('Test context was not initialized; unable to load context environment variable');
        }

        const context = JSON.parse(contextStr, bigIntReviver) as TestContext;
        this.env = context.environment;
        this.reporter = new Reporter();

        // Note: suite files may be nested, and the "name" here should contain the corresponding portion of the
        // directory path. Example: `ts-integration/tests/contracts/some.test.ts` -> `contracts/some.test.ts`.
        const marker = 'ts-integration/tests/';
        const markerPos = file.lastIndexOf(marker);
        if (markerPos === -1) {
            throw new Error(`Received invalid test suite path: ${file}`);
        }
        const suiteName = file.substring(markerPos + marker.length);

        const suiteWalletPK = context.wallets[suiteName];
        if (!suiteWalletPK) {
            throw new Error(`Wallet for ${suiteName} suite was not provided`);
        }
        this.l1Provider = new EthersRetryProvider(this.env.l1NodeUrl, 'L1', this.reporter);
        this.l2Provider = new RetryProvider(
            {
                url: this.env.l2NodeUrl,
                timeout: 1200 * 1000
            },
            undefined,
            this.reporter
        );

        if (isLocalHost(context.environment.network)) {
            // Setup small polling interval on localhost to speed up tests.
            this.l1Provider.pollingInterval = 100;
            this.l2Provider.pollingInterval = 100;
        } else {
            // Poll less frequently to not make the server sad.
            this.l2Provider.pollingInterval = 5000;
        }

        this.mainWallet = new RetryableWallet(suiteWalletPK, this.l2Provider, this.l1Provider);
    }

    /**
     * Returns whether the network is localhost
     *
     * @returns `true` if the test suite is run on localhost and `false` otherwise.
     */
    isLocalHost(): boolean {
        return isLocalHost(this.env.network);
    }

    /**
     * Returns an instance of the `TestMaster` initialized for the specified suite file.
     *
     * @param localSuitePath Local path to the suite file, e.g. `erc20.test.ts` or `sample/file.test.ts`
     * @returns Constructed `TestMaster` object.
     */
    static getInstance(localSuitePath: string): TestMaster {
        if (TestMaster._instance) {
            return TestMaster._instance;
        }

        TestMaster._instance = new TestMaster(localSuitePath);
        return TestMaster._instance;
    }

    /**
     * Getter for the main (funded) account exclusive to the suite.
     */
    mainAccount(): RetryableWallet {
        return this.mainWallet;
    }

    /**
     * Generates a new random empty account.
     * After the test suite is completed, funds from accounts created via this method
     * are recollected back to the main account.
     */
    newEmptyAccount(): zksync.Wallet {
        const randomPK = ethers.Wallet.createRandom().privateKey;
        const newWallet = new RetryableWallet(randomPK, this.l2Provider, this.l1Provider);
        this.subAccounts.push(newWallet);
        return newWallet;
    }

    /**
     * Getter for the test environment.
     */
    environment(): TestEnvironment {
        return this.env;
    }

    /**
     * Checks if tests are being run in the "fast" mode.
     * "Long" mode is default and includes tests that wait for block finalization.
     * "Fast" mode may be used, for example, on stage when we need to quickly run a set
     * of tests.
     */
    isFastMode(): boolean {
        return process.env['ZK_INTEGRATION_TESTS_FAST_MODE'] === 'true';
    }

    /** Returns a vanilla `ethers` provider for L2 with additional retries and logging. This can be useful to check Web3 API compatibility. */
    ethersProvider(layer: 'L1' | 'L2'): EthersRetryProvider {
        const url = layer === 'L1' ? this.env.l1NodeUrl : this.env.l2NodeUrl;
        const provider = new EthersRetryProvider({ url, timeout: 120_000 }, layer, this.reporter);
        provider.pollingInterval = 1_000; // ms
        return provider;
    }

    /**
     * Deinitialized the context, collecting funds from created account back to the main one.
     */
    async deinitialize() {
        try {
            const promises = await claimEtherBack(this.subAccounts, this.mainWallet.address);
            await Promise.all(promises);
        } catch (err) {
            // We don't want deinitialization to fail the test suite, so just report it.
            console.log(`Test deinitialization failed. Error: {err}`);
        }
    }
}
