import { Command } from 'commander';
import enquirer from 'enquirer';
import { BigNumber, ethers, utils } from 'ethers';
import chalk from 'chalk';
import { announced, init, InitArgs } from './init';
import * as server from './server';
import * as docker from './docker';
import * as db from './database';
import * as env from './env';
import { compileConfig } from './config';
import * as fs from 'fs';
import fetch from 'node-fetch';
import { up } from './up';
import * as Handlebars from 'handlebars';
import { ProverType, setupProver } from './prover_setup';

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

enum ProverTypeOption {
    NONE = 'No (this hyperchain is for testing purposes only)',
    CPU = 'Yes - With a CPU implementation',
    GPU = 'Yes - With a GPU implementation'
}

export interface BasePromptOptions {
    name: string | (() => string);
    type: string | (() => string);
    message: string | (() => string) | (() => Promise<string>);
    initial?: any;
    required?: boolean;
    choices?: string[] | object[];
    skip?: ((state: object) => boolean | Promise<boolean>) | boolean;
}

// An init command that allows configuring and spinning up a new hyperchain network.
async function initHyperchain() {
    await announced('Initializing hyperchain creation', setupConfiguration());

    const deployerPrivateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const governorPrivateKey = process.env.GOVERNOR_PRIVATE_KEY;
    const deployL2Weth = Boolean(process.env.DEPLOY_L2_WETH || false);
    const deployTestTokens = Boolean(process.env.DEPLOY_TEST_TOKENS || false);

    const initArgs: InitArgs = {
        skipSubmodulesCheckout: false,
        skipEnvSetup: true,
        governorPrivateKeyArgs: ['--private-key', governorPrivateKey],
        deployerL2ContractInput: {
            args: ['--private-key', deployerPrivateKey],
            includePaymaster: false,
            includeL2WETH: deployL2Weth
        },
        testTokens: {
            deploy: deployTestTokens,
            args: ['--private-key', deployerPrivateKey, '--envFile', process.env.CHAIN_ETH_NETWORK!]
        },
        validiumMode: false
    };

    await init(initArgs);

    env.mergeInitToEnv();

    console.log(announce(`\nYour hyperchain configuration is available at ${process.env.ENV_FILE}\n`));

    console.log(warning(`\nIf you want to add a prover to your hyperchain, please run zk stack prover-setup now.\n`));

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
        await announced('Setting hyperchain configuration', setHyperchainMetadata());
        await announced('Validating information and balances to deploy hyperchain', checkReadinessToDeploy());
    } else {
        const envName = await selectHyperchainConfiguration();

        env.set(envName);
    }
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
            type: 'input',
            required: true
        },
        {
            message: 'What is your hyperchain id? Make sure this is not used by other chains.',
            name: 'chainId',
            type: 'numeral',
            required: true
        },
        {
            message: 'To which L1 Network will your hyperchain rollup to?',
            name: 'l1Chain',
            type: 'select',
            required: true,
            choices: BASE_NETWORKS
        }
    ];

    const results: any = await enquirer.prompt(questions);

    let deployer, governor, ethOperator, feeReceiver: ethers.Wallet | undefined;
    let feeReceiverAddress, l1Rpc, l1Id, databaseUrl;

    if (results.l1Chain !== BaseNetwork.LOCALHOST) {
        const connectionsQuestions: BasePromptOptions[] = [
            {
                message: 'What is the RPC url for the L1 Network?',
                name: 'l1Rpc',
                type: 'input',
                required: true
            }
        ];

        if (results.l1Chain === BaseNetwork.LOCALHOST_CUSTOM) {
            connectionsQuestions[0].initial = 'http://localhost:8545';

            connectionsQuestions.push({
                message: 'What is network id of your L1 Network?',
                name: 'l1NetworkId',
                type: 'numeral',
                required: true
            });
        }

        connectionsQuestions.push({
            message:
                'What is the connection URL for your Postgress 14 database (format is postgres://<user>:<pass>@<hostname>:<port>/<database>)?',
            name: 'dbUrl',
            type: 'input',
            initial: 'postgres://postgres@localhost/zksync_local',
            required: true
        });

        connectionsQuestions.push({
            message:
                'Do you want to generate new addresses/keys for the Deployer, Governor and ETh Operator, or insert your own keys?',
            name: 'generateKeys',
            type: 'select',
            choices: [GENERATE_KEYS, INSERT_KEYS]
        });

        const connectionsResults: any = await enquirer.prompt(connectionsQuestions);

        l1Rpc = connectionsResults.l1Rpc;
        databaseUrl = connectionsResults.dbUrl;

        if (results.l1Chain === BaseNetwork.LOCALHOST_CUSTOM) {
            l1Id = connectionsResults.l1NetworkId;
        } else {
            l1Id = getL1Id(results.l1Chain);
        }

        if (connectionsResults.generateKeys === GENERATE_KEYS) {
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
                    type: 'password',
                    required: true
                },
                {
                    message: 'Private key of the L1 Governor (the one that can upgrade the contracts)',
                    name: 'governorKey',
                    type: 'password',
                    required: true
                },
                {
                    message: 'Private key of the L1 ETH Operator (the one that rolls up the batches)',
                    name: 'ethOperator',
                    type: 'password',
                    required: true
                },
                {
                    message: 'Address of L2 fee receiver (the one that collects fees)',
                    name: 'feeReceiver',
                    type: 'input',
                    required: true
                }
            ];

            const keyResults: any = await enquirer.prompt(keyQuestions);

            try {
                deployer = new ethers.Wallet(keyResults.deployerKey);
            } catch (e) {
                throw Error(error('Deployer private key is invalid'));
            }

            try {
                governor = new ethers.Wallet(keyResults.governorKey);
            } catch (e) {
                throw Error(error('Governor private key is invalid'));
            }

            try {
                ethOperator = new ethers.Wallet(keyResults.ethOperator);
            } catch (e) {
                throw Error(error('ETH Operator private key is invalid'));
            }

            if (!utils.isAddress(keyResults.feeReceiver)) {
                throw Error(error('Fee Receiver address is not a valid address'));
            }

            feeReceiver = undefined;
            feeReceiverAddress = keyResults.feeReceiver;
        }
    } else {
        l1Rpc = 'http://localhost:8545';
        l1Id = 9;
        databaseUrl = 'postgres://postgres:notsecurepassword@localhost:5432/zksync_local';
        wrapEnvModify('DATABASE_URL', databaseUrl);

        const richWalletsRaw = await fetch(
            'https://raw.githubusercontent.com/matter-labs/local-setup/main/rich-wallets.json'
        );

        const richWallets = await richWalletsRaw.json();

        deployer = new ethers.Wallet(richWallets[0].privateKey);
        governor = new ethers.Wallet(richWallets[1].privateKey);
        ethOperator = new ethers.Wallet(richWallets[2].privateKey);
        feeReceiver = undefined;
        feeReceiverAddress = richWallets[3].address;

        await up('docker-compose-zkstack-common.yml');
        await announced('Ensuring databases are up', db.wait());
    }

    await initializeTestERC20s();
    await initializeWethTokenForHyperchain();

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
                    type: 'input',
                    required: true
                }
            ];

            const etherscanResults: any = await enquirer.prompt(etherscanQuestions);

            wrapEnvModify('MISC_ETHERSCAN_API_KEY', etherscanResults.etherscanKey);
        }
    }

    const environment = getEnv(results.chainName);

    await compileConfig(environment);
    env.set(environment);
    // TODO: Generate url for data-compressor with selected region or fix env variable for keys location
    // PLA-595
    wrapEnvModify('DATABASE_URL', databaseUrl);
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
    wrapEnvModify('ETH_SENDER_SENDER_PROOF_SENDING_MODE', 'SkipEveryProof');

    if (feeReceiver) {
        wrapEnvModify('FEE_RECEIVER_PRIVATE_KEY', feeReceiver.privateKey);
    }

    // For now force delay to 20 seconds to ensure batch execution doesn't not happen in same block as batch proving.
    // This bug will be fixed on the smart contract soon.
    wrapEnvModify('CONTRACTS_VALIDATOR_TIMELOCK_EXECUTION_DELAY', '0');
    wrapEnvModify('ETH_SENDER_SENDER_L1_BATCH_MIN_AGE_BEFORE_EXECUTE_SECONDS', '20');

    env.load();
}

async function setupHyperchainProver() {
    let proverType = ProverTypeOption.NONE;

    const proverQuestions: BasePromptOptions[] = [
        {
            message: 'Which ZK Prover implementation you want for your hyperchain?',
            name: 'prover',
            type: 'select',
            required: true,
            choices: [ProverTypeOption.NONE, ProverTypeOption.CPU, ProverTypeOption.GPU]
        }
    ];

    const proverResults: any = await enquirer.prompt(proverQuestions);

    proverType = proverResults.prover;

    switch (proverType) {
        case ProverTypeOption.NONE:
            wrapEnvModify('ETH_SENDER_SENDER_PROOF_SENDING_MODE', 'SkipEveryProof');
            env.mergeInitToEnv();
            break;
        default:
            await setupProver(proverType === ProverTypeOption.CPU ? ProverType.CPU : ProverType.GPU);
    }
}

function printAddressInfo(name: string, address: string) {
    console.log(title(name));
    console.log(`Address - ${address}`);
    console.log('');
}

async function initializeTestERC20s() {
    // TODO: For now selecting NO breaks server-core deployment, should be always YES or create empty-mock file for v2-core
    // PLA-595
    const questions: BasePromptOptions[] = [
        {
            message:
                'Do you want to deploy some test ERC20s to your hyperchain? NB: Temporary broken, always select YES',
            name: 'deployERC20s',
            type: 'confirm'
        }
    ];

    const results: any = await enquirer.prompt(questions);

    if (results.deployERC20s) {
        wrapEnvModify('DEPLOY_TEST_TOKENS', 'true');
        console.log(
            warning(
                `The addresses for the generated test ECR20 tokens will be available at the /etc/tokens/${getEnv(
                    process.env.CHAIN_ETH_NETWORK!
                )}.json file.`
            )
        );
    }
}

async function initializeWethTokenForHyperchain() {
    const questions: BasePromptOptions[] = [
        {
            message: 'Do you want to deploy Wrapped ETH to your hyperchain?',
            name: 'deployWeth',
            type: 'confirm'
        }
    ];

    const results: any = await enquirer.prompt(questions);

    if (results.deployWeth) {
        wrapEnvModify('DEPLOY_L2_WETH', 'true');

        if (!process.env.DEPLOY_TEST_TOKENS) {
            // Only try to fetch this info if no test tokens will be deployed, otherwise WETH address will be defined later.
            const tokens = getTokens(process.env.CHAIN_ETH_NETWORK!);

            let baseWethToken = tokens.find((token: { symbol: string }) => token.symbol == 'WETH')?.address;

            if (!baseWethToken) {
                const wethQuestions = [
                    {
                        message: 'What is the address of the Wrapped ETH on the base chain?',
                        name: 'l1Weth',
                        type: 'input',
                        required: true
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
        }
    }
}

async function startServer() {
    const YES_DEFAULT = 'Yes (default components)';
    const YES_CUSTOM = 'Yes (custom components)';
    const NO = 'Not right now (you can now configure prover, generate docker files, or just run the server later)';

    const questions: BasePromptOptions[] = [
        {
            message: 'Do you want to start your hyperchain server now?',
            name: 'start',
            type: 'select',
            choices: [YES_DEFAULT, YES_CUSTOM, NO]
        }
    ];

    const results: any = await enquirer.prompt(questions);

    let components: string[] = [];
    const defaultChoices = ['http_api', 'eth', 'state_keeper', 'housekeeper', 'tree'];

    if (results.start === NO) {
        return;
    } else if (results.start === YES_CUSTOM) {
        const componentQuestions: BasePromptOptions[] = [
            {
                message: 'Please select the desired components',
                name: 'components',
                type: 'multiselect',
                choices: ['api', 'ws_api', ...defaultChoices].sort()
            }
        ];

        components = ((await enquirer.prompt(componentQuestions)) as any).components;
    } else {
        components = defaultChoices;
    }

    await server.server(false, false, components.join(','));
}

// The current env.modify requires to write down the variable name twice. This wraps it so the caller only writes the name and the value.
export function wrapEnvModify(variable: string, assignedVariable: string) {
    env.modify(variable, `${variable}=${assignedVariable}`);
}

// Make sure all env information is available and wallets are funded.
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
            console.log('Exiting hyperchain initializer.');
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
        .normalize('NFKD') // Split accented characters into their base characters and diacritical marks.
        .replace(/[\u0300-\u036f]/g, '') // Remove all the accents, which happen to be all in the \u03xx UNICODE block.
        .trim() // Trim leading or trailing whitespace.
        .toLowerCase() // Convert to lowercase.
        .replace(/[^a-z0-9 -]/g, '') // Remove non-alphanumeric characters.
        .replace(/\s+/g, '-') // Replace spaces with hyphens.
        .replace(/-+/g, '-'); // Remove consecutive hyphens.
}

type L1Token = {
    name: string;
    symbol: string;
    decimals: number;
    address: string;
};

export function getTokens(network: string): L1Token[] {
    const configPath = `${process.env.ZKSYNC_HOME}/etc/tokens/${network}.json`;
    if (!fs.existsSync(configPath)) {
        return [];
    }
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

async function selectHyperchainConfiguration() {
    const envs = env.getAvailableEnvsFromFiles();

    const envQuestions = [
        {
            message: 'Which hyperchain configuration do you want to use?',
            name: 'env',
            type: 'select',
            choices: [...envs].sort()
        }
    ];

    const envResults: any = await enquirer.prompt(envQuestions);
    return envResults.env;
}

async function generateDockerImages(cmd: Command) {
    await _generateDockerImages(cmd.customDockerOrg);
}

async function _generateDockerImages(_orgName?: string) {
    console.log(warning(`\nThis process will build the docker images and it can take a while. Please be patient.\n`));
    const envName = await selectHyperchainConfiguration();
    env.set(envName);
    const orgName = _orgName || envName;

    await docker.customBuildForHyperchain('server-v2', orgName);

    console.log(warning(`\nDocker image for server created: Server image: ${orgName}/server-v2:latest\n`));

    let hasProver = false;
    let hasGPUProver = false;
    let hasCPUProver = false;
    let needBuildProver = false;
    let artifactsPath, proverSetupArtifacts;
    let witnessVectorGensCount = 0;
    let cudaArch = '';

    if (process.env.ETH_SENDER_SENDER_PROOF_SENDING_MODE !== 'SkipEveryProof') {
        hasProver = true;
        if (process.env.OBJECT_STORE_MODE === 'FileBacked') {
            artifactsPath = process.env.OBJECT_STORE_FILE_BACKED_BASE_PATH;
            proverSetupArtifacts = process.env.FRI_PROVER_SETUP_DATA_PATH;
        }

        if (process.env.PROVER_TYPE === ProverType.GPU) {
            hasGPUProver = true;
            const cudaArchPrompt: BasePromptOptions[] = [
                {
                    message:
                        'What is your GPU Compute Capability version? You can find it in table here - https://en.wikipedia.org/wiki/CUDA#GPUs_supported. Input only 2 numbers withous dot, e.g. if you have RTX 3070 -> Compute Capability 8.6 -> Answer is 86',
                    name: 'cudaArch',
                    type: 'input',
                    required: true
                }
            ];
            const cudaRes: any = await enquirer.prompt(cudaArchPrompt);
            cudaArch = cudaRes.cudaArch;
        } else {
            hasCPUProver = true;
        }

        // TODO: Make this param configurable
        // We need to generate at least 4 witnes-vector-generators per prover, but it can be less, and can be more
        // PLA-683
        witnessVectorGensCount = 4;

        // For Now use only the public images. Too soon to allow prover to be customized
        // await docker.customBuildForHyperchain('witness-generator', orgName);
        // await docker.customBuildForHyperchain('witness-vector-generator', orgName);
        // await docker.customBuildForHyperchain('prover-fri-gateway', orgName);
        // await docker.customBuildForHyperchain('proof-fri-compressor', orgName);
        // if (process.env.PROVER_TYPE === ProverType.CPU) {
        //     isCPUProver = true;
        //     await docker.customBuildForHyperchain('prover-fri', orgName);
        // } else {
        //     await docker.customBuildForHyperchain('witness-vector-generator', orgName);
        //     await docker.customBuildForHyperchain('prover-gpu-fri', orgName);
        // }
    }

    // TODO: Autodetect version via nvidia-smi
    // We have precompiled GPU prover image only for CUDA arch 89 aka ADA, all others need to be re-build
    // PLA-682
    if (process.env.PROVER_TYPE === ProverType.GPU && cudaArch != '89') {
        needBuildProver = true;
    }

    const composeArgs = {
        envFilePath: `./etc/env/${envName}.env`,
        orgName,
        hasProver,
        artifactsPath,
        proverSetupArtifacts,
        hasGPUProver,
        hasCPUProver,
        cudaArch,
        needBuildProver,
        witnessVectorGensCount
    };

    // Creating simple handlebars helper "if (foo AND bar)" to reduce copypaste in compose template
    Handlebars.registerHelper(
        'ifAnd',
        function (this: boolean, a: boolean, b: boolean, options: Handlebars.HelperOptions) {
            if (a && b) {
                return options.fn(this);
            }
            return options.inverse(this);
        }
    );

    const templateFileName = './etc/hyperchains/docker-compose-hyperchain-template.hbs';
    const templateString = fs.existsSync(templateFileName) && fs.readFileSync(templateFileName).toString().trim();
    const template = Handlebars.compile(templateString);
    const result = template(composeArgs);

    fs.writeFileSync(`hyperchain-${envName}.yml`, result);

    console.log(
        announce(
            `Docker images generated successfully, and compose file generate (hyperchain-${envName}.yml). Run the images with "docker compose -f hyperchain-${envName}.yml up -d".\n\n`
        )
    );
}

async function configDemoHyperchain(cmd: Command) {
    fs.existsSync('/etc/env/demo.env') && fs.unlinkSync('/etc/env/demo.env');
    fs.existsSync('/etc/hyperchains/hyperchain-demo.yml') && fs.unlinkSync('/etc/hyperchains/hyperchain-demo.yml');
    await compileConfig('demo');
    env.set('demo');

    wrapEnvModify('CHAIN_ETH_ZKSYNC_NETWORK', 'Zeek hyperchain');
    wrapEnvModify('CHAIN_ETH_ZKSYNC_NETWORK_ID', '1337');
    wrapEnvModify('ETH_SENDER_SENDER_PROOF_SENDING_MODE', 'SkipEveryProof');
    wrapEnvModify('ETH_SENDER_SENDER_L1_BATCH_MIN_AGE_BEFORE_EXECUTE_SECONDS', '20');

    const richWalletsRaw = await fetch(
        'https://raw.githubusercontent.com/matter-labs/local-setup/main/rich-wallets.json'
    );

    const richWallets = await richWalletsRaw.json();

    const deployer = new ethers.Wallet(richWallets[0].privateKey);
    const governor = new ethers.Wallet(richWallets[1].privateKey);

    wrapEnvModify('DEPLOYER_PRIVATE_KEY', deployer.privateKey);
    wrapEnvModify('GOVERNOR_PRIVATE_KEY', governor.privateKey);
    wrapEnvModify('GOVERNOR_ADDRESS', governor.address);

    env.load();

    const deployerPrivateKey = process.env.DEPLOYER_PRIVATE_KEY;
    const governorPrivateKey = process.env.GOVERNOR_PRIVATE_KEY;
    const deployL2Weth = Boolean(process.env.DEPLOY_L2_WETH || false);
    const deployTestTokens = Boolean(process.env.DEPLOY_TEST_TOKENS || true);

    const initArgs: InitArgs = {
        skipSubmodulesCheckout: false,
        skipEnvSetup: cmd.skipEnvSetup,
        governorPrivateKeyArgs: ['--private-key', governorPrivateKey],
        deployerL2ContractInput: {
            args: ['--private-key', deployerPrivateKey],
            includePaymaster: false,
            includeL2WETH: deployL2Weth
        },
        testTokens: {
            deploy: deployTestTokens,
            args: ['--private-key', deployerPrivateKey, '--envFile', process.env.CHAIN_ETH_NETWORK!]
        },
        validiumMode: false
    };

    if (!cmd.skipEnvSetup) {
        await up();
    }
    await init(initArgs);

    env.mergeInitToEnv();

    if (cmd.prover) {
        await setupProver(cmd.prover === 'gpu' ? ProverType.GPU : ProverType.CPU);
    }
}

function printReadme() {
    console.log(
        title(
            '-----------------------------------\nWelcome to ZK Stack hyperchain CLI\n-----------------------------------\n'
        )
    );

    console.log(
        announce('Please follow these steps/commands to get your hyperchain tailored to your (and your users) needs.\n')
    );

    console.log(
        `${chalk.bgBlueBright('zk stack init')} ${chalk.blueBright('- Wizard for hyperchain creation/configuration')}`
    );
    console.log(
        `${chalk.bgBlueBright('zk stack prover-setup')} ${chalk.blueBright(
            '- Configure the ZK Prover instance for your hyperchain'
        )}`
    );
    console.log(
        `${chalk.bgBlueBright('zk stack docker-setup')} ${chalk.blueBright(
            '- Generate docker images and compose file for your hyperchain'
        )}`
    );
    console.log(
        `${chalk.bgBlueBright('zk stack demo')} ${chalk.blueBright(
            '- Spin up a demo hyperchain with default settings for testing purposes'
        )}`
    );

    console.log('\n');
}

export const initHyperchainCommand = new Command('stack')
    .description('ZK Stack hyperchains management')
    .action(printReadme);

initHyperchainCommand
    .command('init')
    .description('Wizard for hyperchain creation/configuration')
    .action(initHyperchain);
initHyperchainCommand
    .command('docker-setup')
    .option('--custom-docker-org <value>', 'Custom organization name for the docker images')
    .description('Generate docker images and compose file for your hyperchain')
    .action(generateDockerImages);
initHyperchainCommand
    .command('prover-setup')
    .description('Configure the ZK Prover instance for your hyperchain')
    .action(setupHyperchainProver);
initHyperchainCommand
    .command('demo')
    .option('--prover <value>', 'Add a cpu or gpu prover to the hyperchain')
    .option('--skip-env-setup', 'Run env setup automatically (pull docker containers, etc)')
    .description('Spin up a demo hyperchain with default settings for testing purposes')
    .action(configDemoHyperchain);
