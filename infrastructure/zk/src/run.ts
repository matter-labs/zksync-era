import { Command } from 'commander';
import * as utils from 'utils';
import { Wallet } from 'ethers';
import fs from 'fs';
import * as path from 'path';
import { getTokens } from './hyperchain_wizard';
import * as env from './env';

export async function deployERC20AndWeth({
    command,
    name,
    symbol,
    decimals,
    envFile
}: {
    command: 'dev' | 'new';
    name?: string;
    symbol?: string;
    decimals?: string;
    envFile?: string;
}) {
    if (command == 'dev') {
        const destinationFile = envFile || 'localhost';
        const privateKey = process.env.DEPLOYER_PRIVATE_KEY;
        const args = [privateKey ? `--private-key ${privateKey}` : ''];
        await utils.spawn(`yarn --silent --cwd contracts/l1-contracts deploy-erc20 add-multi '
            [
                { "name": "DAI",  "symbol": "DAI",  "decimals": 18 },
                { "name": "wBTC", "symbol": "wBTC", "decimals":  8, "implementation": "RevertTransferERC20" },
                { "name": "BAT",  "symbol": "BAT",  "decimals": 18 },
                { "name": "Wrapped Ether", "symbol": "WETH", "decimals": 18, "implementation": "WETH9"}
            ]' ${args.join(' ')} > ./etc/tokens/${destinationFile}.json`);
        const WETH = getTokens(destinationFile).find((token) => token.symbol === 'WETH')!;
        env.modify(
            'CONTRACTS_L1_WETH_TOKEN_ADDR',
            `CONTRACTS_L1_WETH_TOKEN_ADDR=${WETH.address}`,
            `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`
        );
    } else if (command == 'new') {
        await utils.spawn(
            `yarn --silent --cwd contracts/l1-contracts deploy-erc20 add --token-name ${name} --symbol ${symbol} --decimals ${decimals}`
        );
    }
}

export type Token = {
    address: string | null;
    name: string;
    symbol: string;
    decimals: number;
};

export async function tokenInfo(address: string) {
    await utils.spawn(`yarn l1-contracts token-info info ${address}`);
}

// installs all dependencies
export async function yarn() {
    await utils.spawn('run_retried yarn install --frozen-lockfile');
}

export async function revertReason(txHash: string, web3url?: string) {
    await utils.spawn(`yarn l1-contracts ts-node scripts/revert-reason.ts ${txHash} ${web3url || ''}`);
}

export async function catLogs(exitCode?: number) {
    utils.allowFailSync(() => {
        console.log('\nSERVER LOGS:\n', fs.readFileSync('server.log').toString());
        console.log('\nPROVER LOGS:\n', fs.readFileSync('dummy_verifier.log').toString());
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

export async function genesisConfigGenerator(...args: string[]) {
    console.log(args);
    await utils.spawn(`cargo run --release --bin genesis_generator -- ${args.join(' ')}`);
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

export async function snapshots_creator() {
    process.chdir(`${process.env.ZKSYNC_HOME}`);
    let logLevel = 'RUST_LOG=snapshots_creator=debug';
    await utils.spawn(`${logLevel} cargo run --bin snapshots_creator --release`);
}

export const command = new Command('run').description('run miscellaneous applications');

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
        await deployERC20AndWeth({ command, name, symbol, decimals });
    });

command
    .command('token-info <address>')
    .description('get symbol, name and decimals parameters from token')
    .action(async (address: string) => {
        await tokenInfo(address);
    });

command
    .command('revert-reason <tx_hash> [web3_url]')
    .description('get the revert reason for ethereum transaction')
    .action(revertReason);

command
    .command('genesis-config-generator [options...]')
    .description('run the genesis-config-generator')
    .allowUnknownOption()
    .action(async (options: string[]) => {
        await genesisConfigGenerator(...options);
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
