import { Command } from 'commander';
import enquirer from 'enquirer';
import { BigNumber, ethers } from 'ethers';
import chalk from 'chalk';
import { announced, submoduleUpdate } from './init';
import * as server from './server';
import * as contract from './contract';
import * as run from './run/run';
import * as compiler from './compiler';
import * as db from './database';
import { clean } from './clean';
import * as env from './env';
import { compileConfig } from './config';
import * as fs from 'fs';
import fetch from 'node-fetch';
import { up } from './up';

const title = chalk.blueBright;
const warning = chalk.yellowBright;
const error = chalk.redBright;
const announce = chalk.yellow;

enum BaseNetwork {
    LOCALHOST = 'localhost (matterlabs/geth)',
    LOCALHOST_CUSTOM = 'localhost (custom)',
    SEPOLIA = 'sepolia',
    GOERLI = 'goerli',
    MAINNET = 'mainnet'
}

interface BasePromptOptions {
    name: string | (() => string);
    type: string | (() => string);
    message: string | (() => string) | (() => Promise<string>);
    initial?: any;
    required?: boolean;
    choices?: string[];
    skip?: ((state: object) => boolean | Promise<boolean>) | boolean;
}

// An init command that allows configuring and spinning up a new Hyperchain network
async function initHyperchain() {
    await announced('Initializing Hyperchain creation', setupConfiguration());

    await announced('Drop postgres db', db.drop());
    await announced('Setup postgres db', db.setup());
    await announced('Clean rocksdb', clean('db'));
    await announced('Clean backups', clean('backups'));
    await announced('Building L1 and L2 contracts', contract.build());

    await announced('Deploy test tokens', initializeTestERC20s());
    await announced('Deploying L1 verifier', contract.deployVerifier([]));
    await announced('Running server genesis setup', server.genesisFromSources());

    const deployerPrivateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const governorPrivateKey = process.env.GOVERNOR_PRIVATE_KEY;
    const governorAddress = process.env.GOVERNOR_ADDRESS;

    await announced(
        'Deploying L1 contracts',
        contract.redeployL1(['--private-key', deployerPrivateKey, '--governor-address', governorAddress])
    );

    await announced('Initializing validator', contract.initializeValidator(['--private-key', governorPrivateKey]));
    await announced('Initialize L1 allow list', contract.initializeL1AllowList(['--private-key', governorPrivateKey]));
    await announced('Deploying L2 contracts', contract.deployL2(['--private-key', deployerPrivateKey], false));

    await announced('Initialize WETH Token', initializeWethTokenForHyperchain());

    env.mergeInitToEnv();

    console.log(announce(`\nYour Hyperchain configuration is available at ${process.env.ENV_FILE}\n`));

    await announced('Start server', startServer());
}

async function setupConfiguration() {
    const CONFIGURE = 'Configure new chain';
    const USE_EXISTING = 'Use existing configuration';
    const questions: BasePromptOptions[] = [
        {
            message: 'Do you want to configure a new chain or use an existing configuration?',
            name: 'config',
            type: 'select',
            choices: [CONFIGURE, USE_EXISTING]
        }
    ];

    const results: any = await enquirer.prompt(questions);

    if (results.config === CONFIGURE) {
        await announced('Setting Hyperchain metadata', setHyperchainMetadata());
        await announced('Validating information and balances to deploy Hyperchain', checkReadinessToDeploy());
    } else {
        const envs = env.getAvailableEnvsFromFiles();

        const envQuestions = [
            {
                message: 'Which environment do you want to use?',
                name: 'env',
                type: 'select',
                choices: [...envs].sort()
            }
        ];

        const envResults: any = await enquirer.prompt(envQuestions);
        env.set(envResults.env);
    }
    await announced('Checkout system-contracts submodule', submoduleUpdate());
    await announced('Compiling JS packages', run.yarn());
    await announced('Compiling system contracts', compiler.compileSystemContracts());
    await announced('Compile l2 contracts', compiler.compileAll());
}

async function setHyperchainMetadata() {
    const BASE_NETWORKS = [
        BaseNetwork.LOCALHOST,
        BaseNetwork.LOCALHOST_CUSTOM,
        BaseNetwork.SEPOLIA,
        BaseNetwork.GOERLI,
        BaseNetwork.MAINNET
    ];
    const GENERATE_KEYS = 'Generate keys';
    const INSERT_KEYS = 'Insert keys';
    const questions: BasePromptOptions[] = [
        {
            message: 'What is your hyperchain name?',
            name: 'chainName',
            type: 'input'
        },
        {
            message: 'What is your hyperchain id? Make sure this is not used by other chains.',
            name: 'chainId',
            type: 'input'
        },
        {
            message: 'To which L1 Network will your hyperchain rollup to?',
            name: 'l1Chain',
            type: 'select',
            choices: BASE_NETWORKS
        }
    ];

    const results: any = await enquirer.prompt(questions);

    let deployer, governor, ethOperator, feeReceiver: ethers.Wallet | undefined;
    let feeReceiverAddress, l1Rpc, l1Id;

    if (results.l1Chain !== BaseNetwork.LOCALHOST) {
        const rpcQuestions: BasePromptOptions[] = [
            {
                message: 'What is the RPC url for the L1 Network?',
                name: 'l1Rpc',
                type: 'input'
            }
        ];

        if (results.l1Chain === BaseNetwork.LOCALHOST_CUSTOM) {
            rpcQuestions.push({
                message: 'What is netowrk id of your L1 Network?',
                name: 'l1NetworkId',
                type: 'input'
            });
        }

        rpcQuestions.push({
            message:
                'Do you want to generate new addresses/keys for the Deployer, Governor and ETh Operator, or insert your own keys?',
            name: 'generateKeys',
            type: 'select',
            choices: [GENERATE_KEYS, INSERT_KEYS]
        });

        const rpcResults: any = await enquirer.prompt(questions);

        l1Rpc = rpcResults.l1Rpc;

        if (results.l1Chain === BaseNetwork.LOCALHOST_CUSTOM) {
            l1Id = rpcResults.l1NetworkId;
        } else {
            l1Id = getL1Id(results.l1Chain);
        }

        if (rpcResults.generateKeys === GENERATE_KEYS) {
            deployer = ethers.Wallet.createRandom();
            governor = ethers.Wallet.createRandom();
            ethOperator = ethers.Wallet.createRandom();
            feeReceiver = ethers.Wallet.createRandom();
            feeReceiverAddress = feeReceiver.address;
        } else {
            const keyQuestions: BasePromptOptions[] = [
                {
                    message: 'Private key of the L1 Deployer (the one that deploys the contracts)',
                    name: 'deployerKey',
                    type: 'password'
                },
                {
                    message: 'Private key of the L1 Governor (the one that can upgrade the contracts)',
                    name: 'governorKey',
                    type: 'password'
                },
                {
                    message: 'Private key of the L1 ETH Operator (the one that rolls up the batches)',
                    name: 'ethOperator',
                    type: 'password'
                },
                {
                    message: 'Address of L2 fee receiver (the one that collects fees)',
                    name: 'feeReceiver',
                    type: 'input'
                }
            ];

            const keyResults: any = await enquirer.prompt(keyQuestions);

            deployer = new ethers.Wallet(keyResults.deployerKey);
            governor = new ethers.Wallet(keyResults.governorKey);
            ethOperator = new ethers.Wallet(keyResults.ethOperator);
            feeReceiver = undefined;
            feeReceiverAddress = keyResults.feeReceiver;
        }
    } else {
        l1Rpc = 'http://localhost:8545';
        l1Id = 9;

        const richWalletsRaw = await fetch(
            'https://raw.githubusercontent.com/matter-labs/local-setup/main/rich-wallets.json'
        );

        const richWallets = await richWalletsRaw.json();

        deployer = new ethers.Wallet(richWallets[0].privateKey);
        governor = new ethers.Wallet(richWallets[1].privateKey);
        ethOperator = new ethers.Wallet(richWallets[2].privateKey);
        feeReceiver = undefined;
        feeReceiverAddress = richWallets[3].address;

        await up();
        await announced('Ensuring databases are up', db.wait());
    }

    console.log('\n');

    printAddressInfo('Deployer', deployer.address);
    printAddressInfo('Governor', governor.address);
    printAddressInfo('ETH Operator', ethOperator.address);
    printAddressInfo('Fee receiver', feeReceiverAddress);

    console.log(
        warning(
            'Private keys for these wallets are available in the .env file for you chain. Make sure that you have a copy in a safe place.\n'
        )
    );

    if (governor.address == deployer.address) {
        throw Error(error('Governor and Deployer cannot be the same'));
    }

    if (results.l1Chain !== BaseNetwork.LOCALHOST_CUSTOM && results.l1Chain !== BaseNetwork.LOCALHOST) {
        const verifyQuestions: BasePromptOptions[] = [
            {
                message: 'Do You want to verify your L1 contracts? You will need a etherscan API key for it.',
                name: 'verify',
                type: 'confirm'
            }
        ];

        const verifyResults: any = await enquirer.prompt(verifyQuestions);

        if (verifyResults.verify) {
            const etherscanQuestions = [
                {
                    message: 'Please provide your Etherscan API Key.',
                    name: 'etherscanKey',
                    type: 'input'
                }
            ];

            const etherscanResults: any = await enquirer.prompt(etherscanQuestions);

            wrapEnvModify('MISC_ETHERSCAN_API_KEY', etherscanResults.etherscanKey);
        }
    }

    const environment = getEnv(results.chainName);

    await compileConfig(environment);
    env.set(environment);

    wrapEnvModify('ETH_CLIENT_CHAIN_ID', l1Id.toString());
    wrapEnvModify('ETH_CLIENT_WEB3_URL', l1Rpc);
    wrapEnvModify('CHAIN_ETH_NETWORK', getL1Name(results.l1Chain));
    wrapEnvModify('CHAIN_ETH_ZKSYNC_NETWORK', results.chainName);
    wrapEnvModify('CHAIN_ETH_ZKSYNC_NETWORK_ID', results.chainId);
    wrapEnvModify('ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY', ethOperator.privateKey);
    wrapEnvModify('ETH_SENDER_SENDER_OPERATOR_COMMIT_ETH_ADDR', ethOperator.address);
    wrapEnvModify('DEPLOYER_PRIVATE_KEY', deployer.privateKey);
    wrapEnvModify('GOVERNOR_PRIVATE_KEY', governor.privateKey);
    wrapEnvModify('GOVERNOR_ADDRESS', governor.address);
    wrapEnvModify('CHAIN_STATE_KEEPER_FEE_ACCOUNT_ADDR', feeReceiverAddress);
    if (feeReceiver) {
        wrapEnvModify('FEE_RECEIVER_PRIVATE_KEY', feeReceiver.privateKey);
    }

    // For now force delay to 20 seconds to ensure batch execution doesn't not happen in same block as batch proving
    // This bug will be fixed on the smart contract soon
    wrapEnvModify('CONTRACTS_VALIDATOR_TIMELOCK_EXECUTION_DELAY', '0');
    wrapEnvModify('ETH_SENDER_SENDER_L1_BATCH_MIN_AGE_BEFORE_EXECUTE_SECONDS', '20');

    env.load();
}

function printAddressInfo(name: string, address: string) {
    console.log(title(name));
    console.log(`Address - ${address}`);
    console.log('');
}

async function initializeTestERC20s() {
    const questions: BasePromptOptions[] = [
        {
            message: 'Do you want to deploy some test ERC20s to your Hyperchain (only use on testing scenarios)?',
            name: 'deployERC20s',
            type: 'confirm'
        }
    ];

    const results: any = await enquirer.prompt(questions);

    if (results.deployERC20s) {
        const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
        await announced(
            'Deploying localhost ERC20 tokens',
            run.deployERC20('dev', '', '', '', [
                '--private-key',
                privateKey,
                '--envFile',
                process.env.CHAIN_ETH_NETWORK!
            ])
        );
        console.log(
            warning(
                `The addresses for the tokens can be found on the /etc/tokens/${getEnv(
                    process.env.CHAIN_ETH_NETWORK!
                )}.json file. The deployer address is the owner of the token contracts.`
            )
        );
    }
}

async function initializeWethTokenForHyperchain() {
    const questions: BasePromptOptions[] = [
        {
            message: 'Do you want to deploy a Wrapped ETH Bridge?',
            name: 'deployWeth',
            type: 'confirm'
        }
    ];

    const results: any = await enquirer.prompt(questions);

    if (results.deployWeth) {
        const tokens = getTokens(process.env.CHAIN_ETH_NETWORK!);

        let baseWethToken = tokens.find((token: { symbol: string }) => token.symbol == 'WETH')?.address;

        if (!baseWethToken) {
            const wethQuestions = [
                {
                    message: 'What is the address of the Wrapped ETH on the base chain?',
                    name: 'l1Weth',
                    type: 'input'
                }
            ];

            const wethResults: any = await enquirer.prompt(wethQuestions);

            baseWethToken = wethResults.l1Weth;

            if (fs.existsSync(`/etc/tokens/${getEnv(process.env.ZKSYNC_ENV!)}.json`)) {
                tokens.push({
                    name: 'Wrapped Ether',
                    symbol: 'WETH',
                    decimals: 18,
                    address: baseWethToken!
                });
                fs.writeFileSync(
                    `/etc/tokens/${getEnv(process.env.ZKSYNC_ENV!)}.json`,
                    JSON.stringify(tokens, null, 4)
                );
            }
        }

        wrapEnvModify('CONTRACTS_L1_WETH_TOKEN_ADDR', baseWethToken!);

        const governorPrivateKey = process.env.GOVERNOR_PRIVATE_KEY;

        await announced(
            'Initializing L2 WETH token',
            contract.initializeWethToken(['--private-key', governorPrivateKey])
        );
    }
}

async function startServer() {
    const YES_DEFAULT = 'Yes (default components)';
    const YES_CUSTOM = 'Yes (custom components)';
    const NO = 'Not right now';

    const questions: BasePromptOptions[] = [
        {
            message: 'Do you want to start your Hyperchain server now?',
            name: 'start',
            type: 'select',
            choices: [YES_DEFAULT, YES_CUSTOM, NO]
        }
    ];

    const results: any = await enquirer.prompt(questions);

    let components: string[] = [];
    const defaultChoices = ['http_api', 'eth', 'data_fetcher', 'state_keeper', 'housekeeper', 'tree_lightweight'];

    if (results.start === NO) {
        return;
    } else if (results.start === YES_CUSTOM) {
        const componentQuestions: BasePromptOptions[] = [
            {
                message: 'Please select the desired components',
                name: 'components',
                type: 'multiselect',
                choices: ['api', 'ws_api', ...defaultChoices, 'tree'].sort()
            }
        ];

        components = ((await enquirer.prompt(componentQuestions)) as any).components;
    } else {
        components = defaultChoices;
    }

    await server.server(false, false, components.join(','));
}

// The current env.modify requires to write down the variable name twice. This wraps it so the caller only writes the name and the value
function wrapEnvModify(variable: string, assignedVariable: string) {
    env.modify(variable, `${variable}=${assignedVariable}`);
}

// Make sure all env information is available and wallets are funded
async function checkReadinessToDeploy() {
    const provider = new ethers.providers.JsonRpcProvider(process.env.ETH_CLIENT_WEB3_URL!);

    const deployer = new ethers.Wallet(process.env.DEPLOYER_PRIVATE_KEY!, provider);
    const governor = new ethers.Wallet(process.env.GOVERNOR_PRIVATE_KEY!, provider);
    const ethOperator = new ethers.Wallet(process.env.ETH_SENDER_SENDER_OPERATOR_PRIVATE_KEY!, provider);

    async function checkAllWalletBalances(): Promise<Boolean> {
        console.log('Checking balances...');
        const checkPromises = [];
        checkPromises.push(checkBalance(deployer, ethers.utils.parseEther('0.3')));
        checkPromises.push(checkBalance(governor, ethers.utils.parseEther('0.1')));
        checkPromises.push(checkBalance(ethOperator, ethers.utils.parseEther('0.5')));
        const results = await Promise.all(checkPromises);
        return results.every((result) => result);
    }

    while (true) {
        if (await checkAllWalletBalances()) {
            return;
        }

        const TRY_AGAIN = 'Try again';
        const EXIT = "I'll restart later";

        const fundQuestions = [
            {
                message:
                    'Please fund the wallets so that they have enough balance (Deployer - 0.3ETH, Governor - 0.1ETH, and ETH Operator - 0.5ETH)?',
                name: 'fund',
                type: 'select',
                choices: [TRY_AGAIN, EXIT]
            }
        ];

        const fundResults: any = await enquirer.prompt(fundQuestions);

        if (fundResults.fund === EXIT) {
            console.log('Exiting Hyperchain initializer.');
            process.exit(0);
        }
    }
}

async function checkBalance(wallet: ethers.Wallet, expectedBalance: BigNumber): Promise<Boolean> {
    const balance = await wallet.getBalance();
    if (balance.lt(expectedBalance)) {
        console.log(
            `Wallet ${
                wallet.address
            } has insufficient funds. Expected ${expectedBalance.toString()}, got ${balance.toString()}`
        );
        return false;
    }
    return true;
}

function getL1Id(baseChain: BaseNetwork) {
    switch (baseChain) {
        case BaseNetwork.LOCALHOST:
            return 9;
        case BaseNetwork.SEPOLIA:
            return 11155111;
        case BaseNetwork.GOERLI:
            return 5;
        case BaseNetwork.MAINNET:
            return 1;
        default:
            throw Error('Unknown base layer chain');
    }
}

function getL1Name(baseChain: BaseNetwork) {
    switch (baseChain) {
        case BaseNetwork.LOCALHOST:
        case BaseNetwork.LOCALHOST_CUSTOM:
            return 'localhost';
        default:
            return baseChain;
    }
}

function getEnv(chainName: string) {
    return String(chainName)
        .normalize('NFKD') // split accented characters into their base characters and diacritical marks
        .replace(/[\u0300-\u036f]/g, '') // remove all the accents, which happen to be all in the \u03xx UNICODE block.
        .trim() // trim leading or trailing whitespace
        .toLowerCase() // convert to lowercase
        .replace(/[^a-z0-9 -]/g, '') // remove non-alphanumeric characters
        .replace(/\s+/g, '-') // replace spaces with hyphens
        .replace(/-+/g, '-'); // remove consecutive hyphens
}

type L1Token = {
    name: string;
    symbol: string;
    decimals: number;
    address: string;
};

export function getTokens(network: string): L1Token[] {
    const configPath = `${process.env.ZKSYNC_HOME}/etc/tokens/${network}.json`;
    try {
        return JSON.parse(
            fs.readFileSync(configPath, {
                encoding: 'utf-8'
            })
        );
    } catch (e) {
        return [];
    }
}

export const initHyperchainCommand = new Command('init-hyperchain')
    .description('Initializes a new hyperchain network')
    .action(initHyperchain);
