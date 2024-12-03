import { Command } from 'commander';
import * as toml from '@iarna/toml';
import * as fs from 'fs';
import deepExtend from 'deep-extend';
import * as env from './env';
import path from 'path';
import dotenv from 'dotenv';
import { unpackStringSemVer } from 'utils';

function loadConfigFile(configPath: string, stack: string[] = []) {
    if (stack.includes(configPath)) {
        console.error(`Cyclic import detected: ${stack.join(' -> ')} -> ${configPath}`);
        process.exit(1);
    }
    stack.push(configPath);
    let absolutePath = path.join('etc/env', configPath);
    if (!fs.existsSync(absolutePath)) {
        console.error(`File ${configPath} not found`);
        process.exit(1);
    }
    const fileContents = fs.readFileSync(absolutePath);
    let config: any;
    if (configPath.endsWith('.env')) {
        config = dotenv.parse(fileContents.toString());
    } else
        try {
            config = toml.parse(fileContents.toString());
        } catch (e: any) {
            console.error(
                `${configPath} load failed: Parsing error on line ${e.line} column ${e.column}: ${e.message}`
            );
            process.exit(1);
        }

    if (Array.isArray(config['__imports__'])) {
        const imports: any[] = config['__imports__'].map((importPath) => {
            if (fs.existsSync(path.join('etc/env', importPath))) {
                return fs.lstatSync(path.join('etc/env', importPath)).isDirectory()
                    ? loadConfigFolder(importPath as string, stack)
                    : loadConfigFile(importPath as string, stack);
            } else {
                return {};
            }
        });
        delete config['__imports__'];
        stack.pop();
        // Order is important
        return deepExtend({}, ...imports, config);
    } else {
        stack.pop();
        return deepExtend({}, config);
    }
}

function loadConfigFolder(dirPath: string, stack: string[] = []) {
    stack.push(dirPath);
    const configs = fs
        .readdirSync(path.join('etc/env', dirPath))
        .filter((file) => file.endsWith('.env') || file.endsWith('.toml'))
        .map((file) => loadConfigFile(`${dirPath}/${file}`, stack));
    stack.pop();
    return deepExtend({}, ...configs);
}

export function collectVariables(config: any, prefix: string = ''): Map<string, string> {
    let variables: Map<string, string> = new Map();

    for (const key in config) {
        const keyUppercase = key.toUpperCase();
        if (typeof config[key] == 'object' && config[key] !== null) {
            // Checking whether it's an array
            if (Array.isArray(config[key])) {
                // Checking whether the array contains dictionary object
                if (typeof config[key][0] === 'object') {
                    // It's an array of objects: parse each element as a separate key-value pair.
                    for (let i = 0; i < config[key].length; i++) {
                        const newPrefix = `${prefix}${keyUppercase}_${i}_`;
                        let nestedEntries = collectVariables(config[key][i], newPrefix);
                        variables = new Map([...variables, ...nestedEntries]);
                    }
                } else {
                    // It's a plain array, so join the elements into a string.
                    const variableName = `${prefix}${keyUppercase}`;
                    variables.set(variableName, `"${config[key].join(',')}"`);
                }
            } else {
                // It's a map object: parse it recursively.
                const newPrefix = `${prefix}${keyUppercase}_`;
                let nestedEntries = collectVariables(config[key], newPrefix);
                variables = new Map([...variables, ...nestedEntries]);
            }
        } else {
            const variableName = prefix ? `${prefix}${keyUppercase}` : keyUppercase;
            variables.set(variableName, config[key]);
        }
    }

    return variables;
}

export function loadConfig(environment?: string): any {
    environment ??= process.env.ZKSYNC_ENV!;

    const configPath = `configs/${environment}.toml`;
    return loadConfigFile(configPath);
}

export function printAllConfigs(environment?: string) {
    const config = loadConfig(environment);
    console.log(`${JSON.stringify(config, null, 2)}`);
}

export function compileConfig(environment?: string) {
    environment ??= process.env.ZKSYNC_ENV!;
    const config = loadConfig(environment);
    const variables = collectVariables(config);

    let outputFileContents = '';
    variables.forEach((value: string, key: string) => {
        outputFileContents += `${key}=${value}\n`;
    });

    const outputFileName = `etc/env/target/${environment}.env`;
    fs.writeFileSync(outputFileName, outputFileContents);
    console.log(`Configs compiled for ${environment}`);
}

export function pushConfig(environment?: string, diff?: string) {
    environment ??= process.env.ZKSYNC_ENV!;
    const l2InitFile = `etc/env/l2-inits/${environment}.init.env`;
    const l1InitFile = `etc/env/l1-inits/${process.env.L1_ENV_NAME ? process.env.L1_ENV_NAME : '.init'}.env`;
    const difference: number = parseInt(diff ? diff : '0');
    env.modify('API_WEB3_JSON_RPC_HTTP_PORT', `${3050 + 2 * difference}`, l2InitFile, false);

    env.modify('API_WEB3_JSON_RPC_HTTP_URL', `http://127.0.0.1:${3050 + 2 * difference}`, l2InitFile, false);
    env.modify('API_WEB3_JSON_RPC_WS_PORT', `${3050 + 1 + 2 * difference}`, l2InitFile, false);
    env.modify('API_WEB3_JSON_RPC_WS_URL', `ws://127.0.0.1:${3050 + 1 + 2 * difference}`, l2InitFile, false);

    env.modify('API_EXPLORER_PORT', `${3070 + 2 * difference}`, l2InitFile, false);
    env.modify('API_EXPLORER_URL', `http://127.0.0.1:${3070 + 2 * difference}`, l2InitFile, false);

    env.modify('API_CONTRACT_VERIFICATION_PORT', `${3070 + 2 * difference}`, l2InitFile, false);
    env.modify('API_CONTRACT_VERIFICATION_URL', `http://127.0.0.1:${3070 + 2 * difference}`, l2InitFile, false);

    env.modify('API_PROMETHEUS_LISTENER_PORT', `${3012 + 2 * difference}`, l2InitFile, false);
    env.modify('API_PROMETHEUS_PUSHGATEWAY_URL', `http://127.0.0.1:${9091 + difference}`, l2InitFile, false);
    env.modify('API_HEALTHCHECK_PORT', `${3071 + 2 * difference}`, l2InitFile, false);
    env.modify(
        'CIRCUIT_SYNTHESIZER_PROMETHEUS_PUSHGATEWAY_URL',
        `http://127.0.0.1:${9091 + difference}`,
        l2InitFile,
        false
    );

    // we want to be able to run multiple chains locally, but not break the CI
    if (!process.env.IN_DOCKER) {
        env.modify('DATABASE_URL', `postgres://postgres:notsecurepassword@localhost/${environment}`, l2InitFile, false);
        env.modify(
            'TEST_DATABASE_URL',
            `postgres://postgres:notsecurepassword@localhost/${environment}_test`,
            l2InitFile,
            false
        );

        env.modify(
            'DATABASE_PROVER_URL',
            `postgres://postgres:notsecurepassword@localhost/prover_${environment}`,
            l2InitFile,
            false
        );
        env.modify(
            'TEST_DATABASE_PROVER_URL',
            `postgres://postgres:notsecurepassword@localhost/prover_${environment}_test`,
            l2InitFile,
            false
        );
    }

    env.modify('DATABASE_STATE_KEEPER_DB_PATH', `./db/${environment}/state_keeper`, l2InitFile, false);
    env.modify('DATABASE_MERKLE_TREE_PATH', `./db/${environment}/tree`, l2InitFile, false);
    env.modify('DATABASE_MERKLE_TREE_BACKUP_PATH', `./db/${environment}/backups`, l2InitFile, false);

    if (process.env.CONTRACTS_DEV_PROTOCOL_VERSION) {
        const minor = unpackStringSemVer(process.env.CONTRACTS_DEV_PROTOCOL_VERSION)[1];
        // Since we are bumping the minor version the patch is reset to 0.
        env.modify(
            'CONTRACTS_GENESIS_PROTOCOL_VERSION',
            `0.${minor + 1}.0`, // The major version is always 0 for now
            l1InitFile,
            false
        );
    }
    env.reload();
}

// used to increase chainId for easy deployment of next hyperchain on shared bridge
export function bumpChainId() {
    // note we bump in the .toml file directly
    const configFile = `etc/env/configs/${process.env.ZKSYNC_ENV!}.toml`;
    env.modify(
        'CHAIN_ETH_ZKSYNC_NETWORK_ID',
        (parseInt(process.env.CHAIN_ETH_ZKSYNC_NETWORK_ID!) + 1).toString(),
        configFile,
        true
    );
}

export const command = new Command('config').description('config management');

command.command('load [environment]').description('load the config for a certain environment').action(printAllConfigs);
command
    .command('compile [environment] [diff]')
    .description('compile the config for a certain environment')
    .option('-d,--diff', 'diff')
    .action((environment: string, diff: string) => {
        compileConfig(environment);

        diff = diff ? diff : '0';
        pushConfig(environment, diff);
    });
