import * as path from 'path';
import * as fs from 'fs';
import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import { DataAvailabityMode, NodeMode, TestEnvironment } from './types';
import { Reporter } from './reporter';
import { L2_BASE_TOKEN_ADDRESS } from 'zksync-ethers/build/utils';

/**
 * Attempts to connect to server.
 * This function returns once connection can be established, or throws an exception in case of timeout.
 * It also waits for L2 ERC20 bridge to be deployed.
 *
 * This function is expected to be called *before* loading an environment via `loadTestEnvironment`,
 * because the latter expects server to be running and may throw otherwise.
 */
export async function waitForServer() {
    const reporter = new Reporter();
    // Server startup may take a lot of time on the staging.
    const attemptIntervalMs = 1000;
    const maxAttempts = 20 * 60; // 20 minutes

    const l2NodeUrl = ensureVariable(
        process.env.ZKSYNC_WEB3_API_URL || process.env.API_WEB3_JSON_RPC_HTTP_URL,
        'L2 node URL'
    );
    const l2Provider = new zksync.Provider(l2NodeUrl);

    reporter.startAction('Connecting to server');
    for (let i = 0; i < maxAttempts; ++i) {
        try {
            await l2Provider.getNetwork(); // Will throw if the server is not ready yet.
            const bridgeAddress = (await l2Provider.getDefaultBridgeAddresses()).sharedL2;
            const code = await l2Provider.getCode(bridgeAddress);
            if (code == '0x') {
                throw Error('L2 ERC20 bridge is not deployed yet, server is not ready');
            }
            reporter.finishAction();
            return;
        } catch (e) {
            reporter.message(`Attempt #${i + 1} to check the server readiness failed`);
            await zksync.utils.sleep(attemptIntervalMs);
        }
    }
    throw new Error('Failed to wait for the server to start');
}

/**
 * Loads the test environment from the env variables.
 */
export async function loadTestEnvironment(): Promise<TestEnvironment> {
    const network = process.env.CHAIN_ETH_NETWORK || 'localhost';

    let mainWalletPK;
    if (network == 'localhost') {
        const testConfigPath = path.join(process.env.ZKSYNC_HOME!, `etc/test_config/constant`);
        const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
        mainWalletPK = ethers.Wallet.fromMnemonic(ethTestConfig.test_mnemonic as string, "m/44'/60'/0'/0/0").privateKey;
    } else {
        mainWalletPK = ensureVariable(process.env.MASTER_WALLET_PK, 'Main wallet private key');
    }

    const l2NodeUrl = ensureVariable(
        process.env.ZKSYNC_WEB3_API_URL || process.env.API_WEB3_JSON_RPC_HTTP_URL,
        'L2 node URL'
    );
    const l2Provider = new zksync.Provider(l2NodeUrl);
    const baseTokenAddress = await l2Provider.getBaseTokenContractAddress();

    const l1NodeUrl = ensureVariable(process.env.L1_RPC_ADDRESS || process.env.ETH_CLIENT_WEB3_URL, 'L1 node URL');
    const wsL2NodeUrl = ensureVariable(
        process.env.ZKSYNC_WEB3_WS_API_URL || process.env.API_WEB3_JSON_RPC_WS_URL,
        'WS L2 node URL'
    );
    const contractVerificationUrl = process.env.ZKSYNC_ENV!.startsWith('ext-node')
        ? process.env.CONTRACT_VERIFIER_URL!
        : ensureVariable(process.env.CONTRACT_VERIFIER_URL, 'Contract verification API');

    const pathToHome = path.join(__dirname, '../../../../');
    const tokens = getTokens(pathToHome, process.env.CHAIN_ETH_NETWORK || 'localhost');
    // wBTC is chosen because it has decimals different from ETH (8 instead of 18).
    // Using this token will help us to detect decimals-related errors.
    // but if it's not available, we'll use the first token from the list.
    let token = tokens.find((token: { symbol: string }) => token.symbol == 'WBTC')!;
    if (!token) {
        token = tokens[0];
    }
    const weth = tokens.find((token: { symbol: string }) => token.symbol == 'WETH')!;
    const baseToken = tokens.find((token: { address: string }) =>
        zksync.utils.isAddressEq(token.address, baseTokenAddress)
    )!;

    // `waitForServer` is expected to be executed. Otherwise this call may throw.
    const l2TokenAddress = await new zksync.Wallet(
        mainWalletPK,
        l2Provider,
        ethers.getDefaultProvider(l1NodeUrl)
    ).l2TokenAddress(token.address);

    const l2WethAddress = await new zksync.Wallet(
        mainWalletPK,
        l2Provider,
        ethers.getDefaultProvider(l1NodeUrl)
    ).l2TokenAddress(weth.address);

    const baseTokenAddressL2 = L2_BASE_TOKEN_ADDRESS;
    const l2ChainId = parseInt(process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!);
    const l1BatchCommitDataGeneratorMode = process.env
        .CHAIN_STATE_KEEPER_L1_BATCH_COMMIT_DATA_GENERATOR_MODE! as DataAvailabityMode;
    let minimalL2GasPrice;
    if (process.env.CHAIN_STATE_KEEPER_MINIMAL_L2_GAS_PRICE !== undefined) {
        minimalL2GasPrice = ethers.BigNumber.from(process.env.CHAIN_STATE_KEEPER_MINIMAL_L2_GAS_PRICE!);
    } else {
        minimalL2GasPrice = ethers.BigNumber.from(0);
    }
    let nodeMode;
    if (process.env.EN_MAIN_NODE_URL !== undefined) {
        nodeMode = NodeMode.External;
    } else {
        nodeMode = NodeMode.Main;
    }

    const validationComputationalGasLimit = parseInt(
        process.env.CHAIN_STATE_KEEPER_VALIDATION_COMPUTATIONAL_GAS_LIMIT!
    );
    const priorityTxMaxGasLimit = parseInt(process.env.CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT!);
    const maxLogsLimit = parseInt(
        process.env.EN_REQ_ENTITIES_LIMIT ?? process.env.API_WEB3_JSON_RPC_REQ_ENTITIES_LIMIT!
    );

    return {
        maxLogsLimit,
        pathToHome,
        priorityTxMaxGasLimit,
        validationComputationalGasLimit,
        nodeMode,
        minimalL2GasPrice,
        l1BatchCommitDataGeneratorMode,
        l2ChainId,
        network,
        mainWalletPK,
        l2NodeUrl,
        l1NodeUrl,
        wsL2NodeUrl,
        contractVerificationUrl,
        erc20Token: {
            name: token.name,
            symbol: token.symbol,
            decimals: token.decimals,
            l1Address: token.address,
            l2Address: l2TokenAddress
        },
        wethToken: {
            name: weth.name,
            symbol: weth.symbol,
            decimals: weth.decimals,
            l1Address: weth.address,
            l2Address: l2WethAddress
        },
        baseToken: {
            name: baseToken?.name || token.name,
            symbol: baseToken?.symbol || token.symbol,
            decimals: baseToken?.decimals || token.decimals,
            l1Address: baseToken?.address || token.address,
            l2Address: baseTokenAddressL2
        }
    };
}

/**
 * Checks that variable is not `undefined`, throws an error otherwise.
 */
function ensureVariable(value: string | undefined, variableName: string): string {
    if (!value) {
        throw new Error(`${variableName} is not defined in the env`);
    }
    return value;
}

type L1Token = {
    name: string;
    symbol: string;
    decimals: number;
    address: string;
};

function getTokens(pathToHome: string, network: string): L1Token[] {
    const configPath = `${pathToHome}/etc/tokens/${network}.json`;
    if (!fs.existsSync(configPath)) {
        return [];
    }
    return JSON.parse(
        fs.readFileSync(configPath, {
            encoding: 'utf-8'
        })
    );
}
