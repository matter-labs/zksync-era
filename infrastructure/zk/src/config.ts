import { Command } from 'commander';
import * as toml from '@iarna/toml';
import * as fs from 'fs';
import deepExtend from 'deep-extend';

const CONFIG_FILES = [
    'api.toml',
    'chain.toml',
    'contract_verifier.toml',
    'contracts.toml',
    'database.toml',
    'eth_client.toml',
    'eth_sender.toml',
    'eth_watch.toml',
    'misc.toml',
    'object_store.toml',
    'nfs.toml',
    'prover.toml',
    'rust.toml',
    'private.toml',
    'fetcher.toml',
    'witness_generator.toml',
    'circuit_synthesizer.toml',
    'prover_group.toml',
    'house_keeper.toml'
];

function loadConfigFile(path: string) {
    const fileContents = fs.readFileSync(path);
    try {
        return toml.parse(fileContents.toString());
    } catch (e: any) {
        console.error(`${path} load failed: Parsing error on line ${e.line} column ${e.column}: ${e.message}`);
        process.exit(1);
    }
}

function collectVariables(config: any, prefix: string = ''): Map<string, string> {
    let variables: Map<string, string> = new Map();

    for (const key in config) {
        const keyUppercase = key.toLocaleUpperCase();
        if (typeof config[key] == 'object' && config[key] !== null && !Array.isArray(config[key])) {
            // It's a map object: parse it recursively.
            // Add a prefix for the child elements:
            // '' -> 'KEY_'; 'KEY_' -> 'KEY_ANOTHER_KEY_'.
            const newPrefix = `${prefix}${keyUppercase}_`;
            const nestedEntries = collectVariables(config[key], newPrefix);
            variables = new Map([...variables, ...nestedEntries]);
        } else {
            const variableName = `${prefix}${keyUppercase}`;
            const value = Array.isArray(config[key]) ? config[key].join(',') : config[key];
            variables.set(variableName, value);
        }
    }

    return variables;
}

function loadConfig(env?: string) {
    env ??= process.env.ZKSYNC_ENV!;
    let config = {};

    for (const configFile of CONFIG_FILES) {
        const localConfig = loadConfigFile(`etc/env/base/${configFile}`);
        deepExtend(config, localConfig);
    }

    const overridesPath = `${process.env.ZKSYNC_HOME}/etc/env/${env}.toml`;
    if (fs.existsSync(overridesPath)) {
        const overrides = loadConfigFile(overridesPath);
        deepExtend(config, overrides);
    }

    return config;
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

    const outputFileName = `etc/env/${environment}.env`;
    fs.writeFileSync(outputFileName, outputFileContents);
    console.log('Configs compiled');
}

export const command = new Command('config').description('config management');

command.command('load [environment]').description('load the config for a certain environment').action(printAllConfigs);
command
    .command('compile [environment]')
    .description('compile the config for a certain environment')
    .action(compileConfig);
