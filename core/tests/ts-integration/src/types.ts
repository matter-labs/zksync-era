export enum NodeMode {
    Main,
    External
}

export enum DataAvailabityMode {
    Rollup = 'Rollup',
    Validium = 'Validium'
}

/**
 * Description of an ERC20 token.
 */
export interface Token {
    name: string;
    symbol: string;
    decimals: bigint;
    l1Address: string;
    l2Address: string;
    l2AddressSecondChain?: string;
    assetId?: string;
}

/**
 * Description of the environment the integration tests are being run in.
 */
export interface TestEnvironment {
    /*
     * Max Logs limit
     */
    maxLogsLimit: number;
    /*
     * Gas limit for priority txs
     */
    priorityTxMaxGasLimit: bigint;
    /*
     * Gas limit for computations
     */
    validationComputationalGasLimit: number;
    /*
     * Minimal gas price of l2
     */
    minimalL2GasPrice: bigint;
    /*
     * Data availability mode
     */
    l1BatchCommitDataGeneratorMode: DataAvailabityMode;
    /*
     * Path to code home directory
     */
    pathToHome: string;
    /**
     * Chain Id of the L2 Network
     */
    l2ChainId: bigint;
    /**
     * Chain Id of the L2 Network for the second chain, used for interop tests.
     * Defaults to the same as `l2ChainId` if not set.
     */
    l2ChainIdSecondChain: bigint | undefined;
    /*
     * Mode of the l2 node
     */
    nodeMode: NodeMode;
    /*
     * L2 node PID
     */
    l2NodePid: number | undefined;
    /*
     * L2 node PID for the second chain, used for interop tests.
     * Defaults to the same as `l2NodePid` if not set.
     */
    l2NodePidSecondChain: number | undefined;
    /**
     * Plaintext name of the L1 network name (i.e. `localhost` or `goerli`).
     */
    network: string;
    /**
     * Private key of the "master" wallet (used to distribute funds to other wallets).
     * Do not use it directly unless you know what you're doing!
     * Use wallets provided through the test context instead.
     */
    mainWalletPK: string;
    /**
     * URL of ZKsync node's HTTP Web3 API.
     */
    l2NodeUrl: string;
    /**
     * URL of ZKsync node's HTTP Web3 API for the second chain, used for interop tests.
     * Defaults to the same as `l2NodeUrl` if not set.
     */
    l2NodeUrlSecondChain: string | undefined;
    /**
     * URL of Ethereum node's HTTP Web3 API.
     */
    l1NodeUrl: string;
    /**
     * URL of ZKsync node's WS Web3 API.
     */
    wsL2NodeUrl: string;
    /**
     * URL of ZKsync node's contract verification API.
     */
    contractVerificationUrl: string;
    /**
     * Description of the "main" ERC20 token used in the tests.
     */
    erc20Token: Token;
    /**
     * Description of the "base" ERC20 token used in the tests.
     */
    baseToken: Token;
    baseTokenSecondChain: Token | undefined;
    healthcheckPort: string;
    timestampAsserterAddress: string;
    timestampAsserterMinTimeTillEndSec: number;
    l2WETHAddress: string | undefined;
}

/**
 * Set of wallets associated with each test suite file.
 * Represents mapping (file name => private key of funded wallet).
 */
export type TestWallets = {
    [testSuiteFile: string]: string;
};

/**
 * Representation of the data required to run a test suite.
 * Includes addresses of APIs, private keys of funded wallets, and other required data.
 *
 * Note: to share this object to each test suite, Jest requires object to be JSON-serializable.
 * This means that it should be a raw object, and not an instance of a class with some associated functions.
 * If you need to add logic to this interface, consider creating a free function that accepts `TestContext`
 * as an argument.
 */
export interface TestContext {
    wallets: TestWallets;
    environment: TestEnvironment;
}

export interface Fee {
    feeBeforeRefund: bigint;
    feeAfterRefund: bigint;
    refund: bigint;
}
