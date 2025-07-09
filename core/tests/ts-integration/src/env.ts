import * as path from 'path';
import * as fs from 'fs';
import * as ethers from 'ethers';
import * as zksync from 'zksync-ethers';
import { DataAvailabityMode, NodeMode, TestEnvironment } from './types';
import { Reporter } from './reporter';
import * as yaml from 'yaml';
import { L2_BASE_TOKEN_ADDRESS } from 'zksync-ethers/build/utils';
import {
    FileConfig,
    loadConfig,
    loadEcosystem,
    shouldLoadConfigFromFile,
    getSecondChainConfig
} from 'utils/build/file-configs';
import { NodeSpawner } from 'utils/src/node-spawner';
import { logsTestPath } from 'utils/build/logs';
import * as nodefs from 'node:fs/promises';
import { exec } from 'utils';
import { getMainWalletPk } from 'highlevel-test-tools/src/wallets';

async function logsPath(chain: string, name: string): Promise<string> {
    return await logsTestPath(chain, 'logs/server/', name);
}

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

/*
    Loads the environment for file based configs.
 */
async function loadTestEnvironmentFromFile(
    fileConfig: FileConfig,
    secondChainFileConfig: FileConfig | undefined
): Promise<TestEnvironment> {
    let chain = fileConfig.chain!;
    const pathToHome = path.join(__dirname, '../../../..');
    let spawnNode = process.env.SPAWN_NODE;
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
    let contracts = loadConfig({ pathToHome, chain, config: 'contracts.yaml' });

    let genesisConfigSecondChain;
    let generalConfigSecondChain;
    let contractsSecondChain;
    if (secondChainFileConfig) {
        genesisConfigSecondChain = loadConfig({
            pathToHome,
            chain: secondChainFileConfig.chain!,
            config: 'genesis.yaml'
        });
        generalConfigSecondChain = loadConfig({
            pathToHome,
            chain: secondChainFileConfig.chain!,
            config: 'general.yaml'
        });
        contractsSecondChain = loadConfig({
            pathToHome,
            chain: secondChainFileConfig.chain!,
            config: 'contracts.yaml'
        });
    } else {
        genesisConfigSecondChain = genesisConfig;
        generalConfigSecondChain = generalConfig;
        contractsSecondChain = contracts;
    }

    const network = ecosystem.l1_network.toLowerCase();
    const chainName = process.env.CHAIN_NAME!!;
    let mainWalletPK = getMainWalletPk(chainName);
    const l2NodeUrl = generalConfig.api.web3_json_rpc.http_url;
    const l2NodeUrlSecondChain = generalConfigSecondChain.api.web3_json_rpc.http_url;
    const l1NodeUrl = secretsConfig.l1.l1_rpc_url;

    const pathToMainLogs = await logsPath(fileConfig.chain!, 'server.log');
    let mainLogs = await nodefs.open(pathToMainLogs, 'a');
    let l2Node;
    let l2NodeSecondChain;
    console.log(`Loading test environment from file: spawnNode: ${spawnNode}, noKill: ${process.env.NO_KILL}`);
    if (spawnNode) {
        // Before starting any actual logic, we need to ensure that the server is running (it may not
        // be the case, for example, right after deployment on stage).
        const autoKill: boolean = process.env.NO_KILL !== 'true';
        if (autoKill) {
            try {
                await exec(`killall -KILL zksync_server`);
            } catch (err) {
                console.log(`ignored error: ${err}`);
            }
        }
        let mainNodeSpawner = new NodeSpawner(pathToHome, mainLogs, fileConfig, {
            enableConsensus: true,
            ethClientWeb3Url: l1NodeUrl,
            apiWeb3JsonRpcHttpUrl: l2NodeUrl,
            baseTokenAddress: contracts.l1.base_token_addr
        });

        await mainNodeSpawner.killAndSpawnMainNode();
        l2Node = mainNodeSpawner.mainNode;

        if (secondChainFileConfig) {
            const secondChainNodeSpawner = new NodeSpawner(pathToHome, mainLogs, secondChainFileConfig, {
                enableConsensus: true,
                ethClientWeb3Url: l1NodeUrl,
                apiWeb3JsonRpcHttpUrl: l2NodeUrlSecondChain,
                baseTokenAddress: contractsSecondChain.l1.base_token_addr
            });

            await secondChainNodeSpawner.killAndSpawnMainNode();
            l2NodeSecondChain = secondChainNodeSpawner.mainNode;
        } else {
            l2NodeSecondChain = l2Node;
        }
    }

    const l2Provider = new zksync.Provider(l2NodeUrl);
    const baseTokenAddress = await l2Provider.getBaseTokenContractAddress();
    const wsL2NodeUrl = generalConfig.api.web3_json_rpc.ws_url;
    const contractVerificationUrl = `http://127.0.0.1:${generalConfig.contract_verifier.port}`;

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

    const baseTokenAddressL2 = L2_BASE_TOKEN_ADDRESS;
    const l2ChainId = BigInt(genesisConfig.l2_chain_id);
    const l2ChainIdSecondChain = BigInt(genesisConfigSecondChain.l2_chain_id);
    const l1BatchCommitDataGeneratorMode = genesisConfig.l1_batch_commit_data_generator_mode as DataAvailabityMode;
    const minimalL2GasPrice = BigInt(generalConfig.state_keeper.minimal_l2_gas_price);

    const validationComputationalGasLimit = parseInt(generalConfig.state_keeper.validation_computational_gas_limit);
    // TODO set it properly
    const priorityTxMaxGasLimit = 72000000n;
    const maxLogsLimit = parseInt(generalConfig.api.web3_json_rpc.req_entities_limit);

    const healthcheckPort = generalConfig.api.healthcheck.port;
    const timestampAsserterAddress = contracts.l2.timestamp_asserter_addr;
    const timestampAsserterMinTimeTillEndSec = parseInt(generalConfig.timestamp_asserter.min_time_till_end_sec);
    const l2WETHAddress = contracts.l2.predeployed_l2_wrapped_base_token_address;
    return {
        maxLogsLimit,
        pathToHome,
        priorityTxMaxGasLimit,
        validationComputationalGasLimit,
        nodeMode,
        minimalL2GasPrice,
        l1BatchCommitDataGeneratorMode,
        l2ChainId,
        l2ChainIdSecondChain,
        network,
        mainWalletPK,
        l2NodeUrl,
        l2NodeUrlSecondChain,
        l2NodePid: l2Node ? l2Node.proc.pid : undefined,
        l2NodePidSecondChain: l2NodeSecondChain ? l2NodeSecondChain.proc.pid : undefined,
        l1NodeUrl,
        wsL2NodeUrl,
        contractVerificationUrl,
        healthcheckPort,
        erc20Token: {
            name: token.name,
            symbol: token.symbol,
            decimals: token.decimals,
            l1Address: token.address,
            l2Address: l2TokenAddress
        },
        baseToken: {
            name: baseToken?.name || token.name,
            symbol: baseToken?.symbol || token.symbol,
            decimals: baseToken?.decimals || token.decimals,
            l1Address: baseToken?.address || token.address,
            l2Address: baseTokenAddressL2
        },
        timestampAsserterAddress,
        timestampAsserterMinTimeTillEndSec,
        l2WETHAddress
    };
}

export async function loadTestEnvironment(): Promise<TestEnvironment> {
    const fileConfig = shouldLoadConfigFromFile();

    if (!fileConfig.loadFromFile) {
        throw new Error('loading test environment from env is no longer supported');
    }
    const secondChainFileConfig = getSecondChainConfig();
    const testEnvironment = await loadTestEnvironmentFromFile(fileConfig, secondChainFileConfig);
    return testEnvironment;
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

function getTokensNew(pathToHome: string): Tokens {
    const configPath = path.join(pathToHome, '/configs/erc20.yaml');
    if (!fs.existsSync(configPath)) {
        throw Error('Tokens config not found');
    }

    const parsedObject = yaml.parse(
        fs.readFileSync(configPath, {
            encoding: 'utf-8'
        }),
        {
            customTags
        }
    );

    for (const key in parsedObject.tokens) {
        parsedObject.tokens[key].decimals = BigInt(parsedObject.tokens[key].decimals);
    }
    return parsedObject;
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
