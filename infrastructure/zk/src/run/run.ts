import { Command } from 'commander';
import * as utils from '../utils';
import { Wallet } from 'ethers';
import fs from 'fs';
import * as path from 'path';
import * as dataRestore from './data-restore';
import { getNativeToken, getTokens } from '../hyperchain_wizard';
import * as env from '../env';
import { IERC20Factory } from 'zksync-web3/build/typechain';
import { Provider } from 'zksync-web3';

export { dataRestore };

const L2Provider = new Provider(process.env.API_WEB3_JSON_RPC_HTTP_URL);
const L1Provider = new Provider(process.env.ETH_CLIENT_WEB3_URL);

export async function deployERC20(
    command: 'dev' | 'new',
    name?: string,
    symbol?: string,
    decimals?: string,
    args: any = []
) {
    if (command == 'dev') {
        let destinationFile = 'localhost';
        if (args.includes('--envFile')) {
            destinationFile = args[args.indexOf('--envFile') + 1];
            args.splice(args.indexOf('--envFile'), 2);
        }
        await utils.spawn(`yarn --silent --cwd contracts/l1-contracts deploy-erc20 add-multi '
            [
                { "name": "DAI",  "symbol": "DAI",  "decimals": 18 },
                { "name": "wBTC", "symbol": "wBTC", "decimals":  8, "implementation": "RevertTransferERC20" },
                { "name": "BAT",  "symbol": "BAT",  "decimals": 18 },
                { "name": "GNT",  "symbol": "GNT",  "decimals": 18 },
                { "name": "MLTT", "symbol": "MLTT", "decimals": 18 },
                { "name": "DAIK",  "symbol": "DAIK",  "decimals": 18 },
                { "name": "wBTCK", "symbol": "wBTCK", "decimals":  8, "implementation": "RevertTransferERC20" },
                { "name": "BATK",  "symbol": "BATS",  "decimals": 18 },
                { "name": "GNTK",  "symbol": "GNTS",  "decimals": 18 },
                { "name": "MLTTK", "symbol": "MLTTS", "decimals": 18 },
                { "name": "DAIL",  "symbol": "DAIL",  "decimals": 18 },
                { "name": "wBTCL", "symbol": "wBTCP", "decimals":  8, "implementation": "RevertTransferERC20" },
                { "name": "BATL",  "symbol": "BATW",  "decimals": 18 },
                { "name": "GNTL",  "symbol": "GNTW",  "decimals": 18 },
                { "name": "MLTTL", "symbol": "MLTTW", "decimals": 18 },
                { "name": "Wrapped Ether", "symbol": "WETH", "decimals": 18, "implementation": "WETH9"}
            ]' ${args.join(' ')} > ./etc/tokens/${destinationFile}.json`);
        const WETH = getTokens(destinationFile).find((token) => token.symbol === 'WETH')!;
        env.modify('CONTRACTS_L1_WETH_TOKEN_ADDR', `CONTRACTS_L1_WETH_TOKEN_ADDR=${WETH.address}`);
    } else if (command == 'new') {
        let destinationFile = 'native_erc20';
        await utils
            .spawn(
                `yarn --silent --cwd contracts/l1-contracts deploy-erc20 add --token-name ${name} --symbol ${symbol} --decimals ${decimals} > ./etc/tokens/${destinationFile}.json`
            )
            .then(() => {
                const NATIVE_ERC20 = getNativeToken();
                env.modify(
                    'CONTRACTS_L1_NATIVE_ERC20_TOKEN_ADDR',
                    `CONTRACTS_L1_NATIVE_ERC20_TOKEN_ADDR=${NATIVE_ERC20.address}`
                );
            });
    }
}

export async function approve() {
    let path = `${process.env.ZKSYNC_HOME}/etc/tokens/native_erc20.json`;
    let rawData = fs.readFileSync(path, 'utf8');
    let address = '0x52312AD6f01657413b2eaE9287f6B9ADaD93D5FE';
    try {
        let jsonConfig = JSON.parse(rawData);
        address = jsonConfig.address;
    } catch (_e) {
        address = '0x52312AD6f01657413b2eaE9287f6B9ADaD93D5FE';
    }

    await utils.spawn(
        `yarn --silent --cwd contracts/l1-contracts deploy-erc20 approve --token-address ${address} --spender-address ${process.env.CONTRACTS_DIAMOND_PROXY_ADDR}`
    );
}

export async function tokenInfo(address: string) {
    await utils.spawn(`yarn l1-contracts token-info info ${address}`);
}

// installs all dependencies
export async function yarn() {
    await utils.spawn('yarn');
}

export async function deployTestkit(genesisRoot: string) {
    await utils.spawn(`yarn l1-contracts deploy-testkit --genesis-root ${genesisRoot}`);
}

export async function revertReason(txHash: string, web3url?: string) {
    await utils.spawn(`yarn l1-contracts ts-node scripts/revert-reason.ts ${txHash} ${web3url || ''}`);
}

export async function exitProof(...args: string[]) {
    await utils.spawn(`cargo run --example generate_exit_proof --release -- ${args.join(' ')}`);
}

export async function catLogs(exitCode?: number) {
    utils.allowFailSync(() => {
        console.log('\nSERVER LOGS:\n', fs.readFileSync('server.log').toString());
        console.log('\nPROVER LOGS:\n', fs.readFileSync('dummy_prover.log').toString());
    });
    if (exitCode !== undefined) {
        process.exit(exitCode);
    }
}

export async function testAccounts() {
    const testConfigPath = path.join(process.env.ZKSYNC_HOME as string, `etc/test_config/constant`);
    const ethTestConfig = JSON.parse(fs.readFileSync(`${testConfigPath}/eth.json`, { encoding: 'utf-8' }));
    const NUM_TEST_WALLETS = 10;
    const baseWalletPath = "m/44'/60'/0'/0/";
    const walletKeys = [];
    for (let i = 0; i < NUM_TEST_WALLETS; ++i) {
        const ethWallet = Wallet.fromMnemonic(ethTestConfig.test_mnemonic as string, baseWalletPath + i);
        walletKeys.push({
            address: ethWallet.address,
            privateKey: ethWallet.privateKey
        });
    }
    console.log(JSON.stringify(walletKeys, null, 4));
}

export async function loadtest(...args: string[]) {
    console.log(args);
    await utils.spawn(`cargo run --release --bin loadnext -- ${args.join(' ')}`);
}

export async function readVariable(address: string, contractName: string, variableName: string, file?: string) {
    if (file === undefined)
        await utils.spawn(
            `yarn --silent --cwd contracts/l1-contracts read-variable read ${address} ${contractName} ${variableName}`
        );
    else
        await utils.spawn(
            `yarn --silent --cwd contracts/l1-contracts read-variable read ${address} ${contractName} ${variableName} -f ${file}`
        );
}

export async function cross_en_checker() {
    let logLevel = 'RUST_LOG=cross_external_nodes_checker=debug';
    let suffix = 'cargo run --release --bin cross_external_nodes_checker';
    await utils.spawn(`${logLevel} ${suffix}`);
}

export async function l1_erc20_balance(tokenAddress: string, walletAddress: string) {
    let token = IERC20Factory.connect(tokenAddress, L1Provider);
    let balance = await token.balanceOf(walletAddress);
    console.log(`Balance is ${balance.toString()}`);
}

export async function snapshots_creator() {
    process.chdir(`${process.env.ZKSYNC_HOME}`);
    let logLevel = 'RUST_LOG=snapshots_creator=debug';
    await utils.spawn(`${logLevel} cargo run --bin snapshots_creator --release`);
}
export const command = new Command('run').description('run miscellaneous applications').addCommand(dataRestore.command);

command.command('test-accounts').description('print ethereum test accounts').action(testAccounts);
command.command('yarn').description('install all JS dependencies').action(yarn);
command.command('cat-logs [exit_code]').description('print server and prover logs').action(catLogs);

command
    .command('deploy-erc20 <dev|new> [name] [symbol] [decimals]')
    .description('deploy ERC20 tokens')
    .action(async (command: string, name?: string, symbol?: string, decimals?: string) => {
        if (command != 'dev' && command != 'new') {
            throw new Error('only "dev" and "new" subcommands are allowed');
        }
        await deployERC20(command, name, symbol, decimals);
    });

command
    .command('token-info <address>')
    .description('get symbol, name and decimals parameters from token')
    .action(async (address: string) => {
        await tokenInfo(address);
    });

command
    .command('l1-erc20-balance <token-address> <wallet-address>')
    .description('get symbol, name and decimals parameters from token')
    .action(async (tokenAddress: string, walletAddress: string) => {
        await l1_erc20_balance(tokenAddress, walletAddress);
    });

command
    .command('deploy-testkit')
    .description('deploy testkit contracts')
    .requiredOption('--genesis-root <hash>')
    .action(async (cmd: Command) => {
        await deployTestkit(cmd.genesisRoot);
    });

command
    .command('revert-reason <tx_hash> [web3_url]')
    .description('get the revert reason for ethereum transaction')
    .action(revertReason);

command
    .command('exit-proof')
    .option('--account <id>')
    .option('--token <id>')
    .option('--help')
    .description('generate exit proof')
    .action(async (cmd: Command) => {
        if (!cmd.account || !cmd.token) {
            await exitProof('--help');
        } else {
            await exitProof('--account_id', cmd.account, '--token', cmd.token);
        }
    });

command
    .command('loadtest [options...]')
    .description('run the loadtest')
    .allowUnknownOption()
    .action(async (options: string[]) => {
        await loadtest(...options);
    });

command
    .command('read-variable <address> <contractName> <variableName>')
    .option(
        '-f --file <file>',
        'file with contract source code(default $ZKSYNC_HOME/contracts/contracts/${contractName}.sol)'
    )
    .description('Read value of contract variable')
    .action(async (address: string, contractName: string, variableName: string, cmd: Command) => {
        await readVariable(address, contractName, variableName, cmd.file);
    });

command.command('snapshots-creator').action(snapshots_creator);

command
    .command('cross-en-checker')
    .description('run the cross external nodes checker. See Checker Readme the default run mode and configuration.')
    .option(
        '--mode <mode>',
        '`Rpc` to run only the RPC checker; `PubSub` to run only the PubSub checker; `All` to run both.'
    )
    .option(
        '--env <env>',
        `Provide the env the checker will test in to use the default urls for that env. 'Local', 'Stage, 'Testnet', or 'Mainnet'`
    )
    .option('--main_node_http_url <url>', 'Manually provide the HTTP URL of the main node')
    .option('--instances_http_urls <urls>', 'Manually provide the HTTP URLs of the instances to check')
    .option('--main_node_ws_url <url>', 'Manually provide the WS URL of the main node')
    .option('--instances_ws_urls <urls>', 'Manually provide the WS URLs of the instances to check')
    .option(
        '--rpc_mode <rpc_mode>',
        'The mode to run the RPC checker in. `Triggered` to run once; `Continuous` to run forever.'
    )
    .option(
        '--start_miniblock <start_miniblock>',
        'Check all miniblocks starting from this. If not set, then check from genesis. Inclusive.'
    )
    .option(
        '--finish_miniblock <finish_miniblock>',
        'For Triggered mode. If not set, then check all available miniblocks. Inclusive.'
    )
    .option(
        '--max_transactions_to_check <max_transactions_to_check>',
        'The maximum number of transactions to be checked at random in each miniblock.'
    )
    .option(
        '--instance_poll_period <instance_poll_period>',
        'For RPC mode. In seconds, how often to poll the instance node for new miniblocks.'
    )
    .option(
        '--subscription_duration <subscription_duration>',
        'For PubSub mode. Time in seconds for a subscription to be active. If not set, then the subscription will run forever.'
    )
    .action(async (cmd: Command) => {
        interface Environment {
            httpMain: string;
            httpInstances: string;
            wsMain: string;
            wsInstances: string;
        }

        const nodeUrls: Record<string, Environment> = {
            Local: {
                httpMain: 'http://127.0.0.1:3050',
                httpInstances: 'http://127.0.0.1:3060',
                wsMain: 'ws://127.0.0.1:3051',
                wsInstances: 'ws://127.0.0.1:3061'
            },
            Stage: {
                httpMain: 'https://z2-dev-api.zksync.dev:443',
                httpInstances: 'https://external-node-dev.zksync.dev:443',
                wsMain: 'wss://z2-dev-api.zksync.dev:443/ws',
                wsInstances: 'wss://external-node-dev.zksync.dev:443/ws'
            },
            Testnet: {
                httpMain: 'https://zksync2-testnet.zksync.dev:443',
                httpInstances: 'https://external-node-testnet.zksync.dev:443',
                wsMain: 'wss://zksync2-testnet.zksync.dev:443/ws',
                wsInstances: 'wss://external-node-testnet.zksync.dev:443/ws'
            },
            Mainnet: {
                httpMain: 'https://zksync2-mainnet.zksync.io:443',
                httpInstances: 'https://external-node-mainnet.zksync.dev:443',
                wsMain: 'wss://zksync2-mainnet.zksync.io:443/ws',
                wsInstances: 'wss://external-node-mainnet.zksync.dev:443/ws'
            }
        };

        if (cmd.env && nodeUrls[cmd.env]) {
            process.env.CHECKER_MAIN_NODE_HTTP_URL = nodeUrls[cmd.env].httpMain;
            process.env.CHECKER_INSTANCES_HTTP_URLS = nodeUrls[cmd.env].httpInstances;
            process.env.CHECKER_MAIN_NODE_WS_URL = nodeUrls[cmd.env].wsMain;
            process.env.CHECKER_INSTANCES_WS_URLS = nodeUrls[cmd.env].wsInstances;
        }

        const envVarMap = {
            mode: 'CHECKER_MODE',
            rpc_mode: 'CHECKER_RPC_MODE',
            main_node_http_url: 'CHECKER_MAIN_NODE_HTTP_URL',
            instances_http_urls: 'CHECKER_INSTANCES_HTTP_URLS',
            main_node_ws_url: 'CHECKER_MAIN_NODE_WS_URL',
            instances_ws_urls: 'CHECKER_INSTANCES_WS_URLS',
            start_miniblock: 'CHECKER_START_MINIBLOCK',
            finish_miniblock: 'CHECKER_FINISH_MINIBLOCK',
            max_transactions_to_check: 'CHECKER_MAX_TRANSACTIONS_TO_CHECK',
            instance_poll_period: 'CHECKER_INSTANCE_POLL_PERIOD',
            subscription_duration: 'CHECKER_SUBSCRIPTION_DURATION'
        };

        for (const [cmdOption, envVar] of Object.entries(envVarMap)) {
            if (cmd[cmdOption]) {
                process.env[envVar] = cmd[cmdOption];
            }
        }

        await cross_en_checker();
    });
