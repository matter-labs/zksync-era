import { Command } from 'commander';
import * as utils from '../utils';
import { Wallet } from 'ethers';
import fs from 'fs';
import * as path from 'path';
import * as dataRestore from './data-restore';
import { getTokens } from '../hyperchain_wizard';
import * as env from '../env';

export { dataRestore };

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
        await utils.spawn(
            `yarn --silent --cwd contracts/l1-contracts deploy-erc20 add --token-name ${name} --symbol ${symbol} --decimals ${decimals}`
        );
    }
}

export async function tokenInfo(address: string) {
    await utils.spawn(`yarn l1-contracts token-info info ${address}`);
}

// installs all dependencies
export async function yarn() {
    await utils.spawn('yarn install --frozen-lockfile');
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

export async function snapshots_creator() {
    process.chdir(`${process.env.ZKSYNC_HOME}`);
    let logLevel = 'RUST_LOG=snapshots_creator=debug';
    await utils.spawn(`${logLevel} cargo run --bin snapshots_creator --release`);
}
export const command = new Command('run').description('run miscellaneous applications').addCommand(dataRestore.command);

command.command('test-accounts').description('print ethereum test accounts').action(testAccounts);
command.command('yarn install --frozen-lockfile').description('install all JS dependencies').action(yarn);
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
