import * as path from 'path';
import * as fs from 'fs';
import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import { DataAvailabityMode, NodeMode, TestEnvironment } from './types';
import { Reporter } from './reporter';
import * as yaml from 'yaml';
import { L2_BASE_TOKEN_ADDRESS } from 'zksync-ethers/build/utils';
import { loadConfig, loadEcosystem, shouldLoadConfigFromFile } from 'utils/build/file-configs';

/**
 * Attempts to connect to server.
 * This function returns once connection can be established, or throws an exception in case of timeout.
 * It also waits for L2 ERC20 bridge to be deployed.
 *
 * This function is expected to be called *before* loading an environment via `loadTestEnvironment`,
 * because the latter expects server to be running and may throw otherwise.
 */
export async function waitForServer(l2NodeUrl: string) {
    const reporter = new Reporter();
    // Server startup may take a lot of time on the staging.
    const attemptIntervalMs = 1000;
    const maxAttempts = 20 * 60; // 20 minutes

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

function getMainWalletPk(pathToHome: string, network: string): string {
    if (network.toLowerCase() == 'localhost') {
        const testConfigPath = path.join(pathToHome, `etc/test_config/constant`);
        const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
        return ethers.Wallet.fromPhrase(ethTestConfig.test_mnemonic).privateKey;
    } else {
        return ensureVariable(process.env.MASTER_WALLET_PK, 'Main wallet private key');
    }
}

/*
    Loads the environment for file based configs.
 */
async function loadTestEnvironmentFromFile(chain: string): Promise<TestEnvironment> {
    const pathToHome = path.join(__dirname, '../../../..');
    let nodeMode;
    if (process.env.EXTERNAL_NODE == 'true') {
        nodeMode = NodeMode.External;
    } else {
        nodeMode = NodeMode.Main;
    }
    let ecosystem = loadEcosystem(pathToHome);
    // Genesis file is common for both EN and Main node
    let genesisConfig = loadConfig({ pathToHome, chain, config: 'genesis.yaml' });

    let configsFolderSuffix = nodeMode == NodeMode.External ? 'external_node' : undefined;
    let generalConfig = loadConfig({ pathToHome, chain, config: 'general.yaml', configsFolderSuffix });
    let secretsConfig = loadConfig({ pathToHome, chain, config: 'secrets.yaml', configsFolderSuffix });

    const network = ecosystem.l1_network;
    let mainWalletPK = getMainWalletPk(pathToHome, network);
    const l2NodeUrl = generalConfig.api.web3_json_rpc.http_url;

    await waitForServer(l2NodeUrl);

    const l2Provider = new zksync.Provider(l2NodeUrl);
    const baseTokenAddress = await l2Provider.getBaseTokenContractAddress();

    const l1NodeUrl = secretsConfig.l1.l1_rpc_url;
    const wsL2NodeUrl = generalConfig.api.web3_json_rpc.ws_url;

    const contractVerificationUrl = generalConfig.contract_verifier.url;

    const tokens = getTokensNew(pathToHome);
    // wBTC is chosen because it has decimals different from ETH (8 instead of 18).
    // Using this token will help us to detect decimals-related errors.
    // but if it's not available, we'll use the first token from the list.
    let token = tokens.tokens['WBTC'];
    if (token === undefined) {
        token = Object.values(tokens.tokens)[0];
        if (token.symbol == 'WETH') {
            token = Object.values(tokens.tokens)[1];
        }
    }
    const weth = tokens.tokens['WETH'];
    let baseToken;

    for (const key in tokens.tokens) {
        const token = tokens.tokens[key];
        if (zksync.utils.isAddressEq(token.address, baseTokenAddress)) {
            baseToken = token;
        }
    }
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
    const l2ChainId = BigInt(genesisConfig.l2_chain_id);
    const l1BatchCommitDataGeneratorMode = genesisConfig.l1_batch_commit_data_generator_mode as DataAvailabityMode;
    const minimalL2GasPrice = BigInt(generalConfig.state_keeper.minimal_l2_gas_price);

    const validationComputationalGasLimit = parseInt(generalConfig.state_keeper.validation_computational_gas_limit);
    // TODO set it properly
    const priorityTxMaxGasLimit = 72000000n;
    const maxLogsLimit = parseInt(generalConfig.api.web3_json_rpc.req_entities_limit);

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

export async function loadTestEnvironment(): Promise<TestEnvironment> {
    const { loadFromFile, chain } = shouldLoadConfigFromFile();

    if (loadFromFile) {
        return await loadTestEnvironmentFromFile(chain);
    }
    return await loadTestEnvironmentFromEnv();
}

/**
 * Loads the test environment from the env variables.
 */
export async function loadTestEnvironmentFromEnv(): Promise<TestEnvironment> {
    const network = process.env.CHAIN_ETH_NETWORK || 'localhost';
    const pathToHome = path.join(__dirname, '../../../../');

    let mainWalletPK = getMainWalletPk(pathToHome, network);

    const l2NodeUrl = ensureVariable(
        process.env.ZKSYNC_WEB3_API_URL || process.env.API_WEB3_JSON_RPC_HTTP_URL,
        'L2 node URL'
    );

    await waitForServer(l2NodeUrl);
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
    const l2ChainId = BigInt(process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!);
    // If the `CHAIN_STATE_KEEPER_L1_BATCH_COMMIT_DATA_GENERATOR_MODE` is not set, the default value is `Rollup`.
    const l1BatchCommitDataGeneratorMode = (process.env.CHAIN_STATE_KEEPER_L1_BATCH_COMMIT_DATA_GENERATOR_MODE ||
        process.env.EN_L1_BATCH_COMMIT_DATA_GENERATOR_MODE ||
        'Rollup') as DataAvailabityMode;
    let minimalL2GasPrice;
    if (process.env.CHAIN_STATE_KEEPER_MINIMAL_L2_GAS_PRICE !== undefined) {
        minimalL2GasPrice = BigInt(process.env.CHAIN_STATE_KEEPER_MINIMAL_L2_GAS_PRICE!);
    } else {
        minimalL2GasPrice = 0n;
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
    const priorityTxMaxGasLimit = BigInt(process.env.CONTRACTS_PRIORITY_TX_MAX_GAS_LIMIT!);
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

interface TokensDict {
    [key: string]: L1Token;
}

type Tokens = {
    tokens: TokensDict;
};

type L1Token = {
    name: string;
    symbol: string;
    decimals: bigint;
    address: string;
};

function getTokens(pathToHome: string, network: string): L1Token[] {
    const configPath = `${pathToHome}/etc/tokens/${network}.json`;
    if (!fs.existsSync(configPath)) {
        return [];
    }
    const parsed = JSON.parse(
        fs.readFileSync(configPath, {
            encoding: 'utf-8'
        }),
        (key, value) => (key === 'decimals' ? BigInt(value) : value)
    );
    return parsed;
}

function getTokensNew(pathToHome: string): Tokens {
    const configPath = path.join(pathToHome, '/configs/erc20.yaml');
    if (!fs.existsSync(configPath)) {
        throw Error('Tokens config not found');
    }

    return yaml
        .parse(
            fs.readFileSync(configPath, {
                encoding: 'utf-8'
            }),
            {
                customTags
            }
        )
        .map((token: any) => {
            return {
                ...token,
                decimals: BigInt(token.decimals)
            };
        });
}

function customTags(tags: yaml.Tags): yaml.Tags {
    for (const tag of tags) {
        // @ts-ignore
        if (tag.format === 'HEX') {
            // @ts-ignore
            tag.resolve = (str, _onError, _opt) => {
                return str;
            };
        }
    }
    return tags;
}
