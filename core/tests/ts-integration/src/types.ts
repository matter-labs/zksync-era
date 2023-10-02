import { ethers } from 'ethers';

/**
 * Description of an ERC20 token.
 */
export interface Token {
    name: string;
    symbol: string;
    decimals: number;
    l1Address: string;
    l2Address: string;
}

/**
 * Description of the environment the integration tests are being run in.
 */
export interface TestEnvironment {
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
     * URL of zkSync node's HTTP Web3 API.
     */
    l2NodeUrl: string;
    /**
     * URL of Ethereum node's HTTP Web3 API.
     */
    l1NodeUrl: string;
    /**
     * URL of zkSync node's WS Web3 API.
     */
    wsL2NodeUrl: string;
    /**
     * URL of zkSync node's contract verification API.
     */
    contractVerificationUrl: string;
    /**
     * Description of the "main" ERC20 token used in the tests.
     */
    erc20Token: Token;
    /**
     * Description of the WETH token used in the tests.
     */
    wethToken: Token;
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
    feeBeforeRefund: ethers.BigNumber;
    feeAfterRefund: ethers.BigNumber;
    refund: ethers.BigNumber;
}
