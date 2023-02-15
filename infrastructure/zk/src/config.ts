import { Command } from 'commander';
import * as toml from '@iarna/toml';
import * as fs from 'fs';
import * as path from 'path';
import deepExtend from 'deep-extend';
import { env } from 'process';

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
    'prover_group.toml'
];

async function getEnvironment(): Promise<string> {
    const environmentFilePath = path.join(envDirPath(), 'current');
    // Try to read environment from file.
    if (fs.existsSync(environmentFilePath)) {
        const environment = (await fs.promises.readFile(environmentFilePath)).toString().trim();
        if (environment !== '') {
            return environment;
        }
    }

    // Fallback scenario: file doesn't exist or is empty.
    return 'dev';
}

function envDirPath(): string {
    return path.join(env['ZKSYNC_HOME'] as string, 'etc', 'env');
}

function getConfigPath(environment: string, configName: string): string {
    return path.join(envDirPath(), environment, configName);
}

async function loadConfig(environment: string, configName: string) {
    const configPath = getConfigPath(environment, configName);
    const fileContents = await fs.promises.readFile(configPath);
    try {
        return toml.parse(fileContents.toString());
    } catch (e: any) {
        console.error(
            `<${environment}/${configName}> load failed: Parsing error on line ${e.line} column ${e.column}: ${e.message}`
        );
        process.exit(1);
    }
}

async function checkConfigExistence(environment: string) {
    const configFolder = path.join(envDirPath(), environment);

    // Check if the folder exists and it's not empty.
    if (fs.existsSync(configFolder) && fs.readdirSync(configFolder).length != 0) {
        return;
    }

    // Folder doesn't exist or it's empty.
    if (environment == 'dev') {
        // Copy configs from the `base` folder.
        // Folder may be created, just be empty, so create it only if needed.
        if (!fs.existsSync(configFolder)) {
            await fs.promises.mkdir(configFolder);
        }

        for (const configFile of CONFIG_FILES) {
            const from = getConfigPath('base', configFile);
            const to = getConfigPath('dev', configFile);
            await fs.promises.copyFile(from, to);
        }
        return;
    }

    // Folder doesn't exist and the environment is not `dev`.
    console.error(`Configuration files were not found for environment <${environment}>`);
    process.exit(1);
}

function collectVariables(prefix: string, config: any): Map<string, string> {
    let variables: Map<string, string> = new Map();

    for (const key in config) {
        const keyUppercase = key.toLocaleUpperCase();
        if (typeof config[key] == 'object' && config[key] !== null && !Array.isArray(config[key])) {
            // It's a map object: parse it recursively.

            // Add a prefix for the child elements:
            // '' -> 'KEY_'; 'KEY_' -> 'KEY_ANOTHER_KEY_'.
            const newPrefix = `${prefix}${keyUppercase}_`;

            const nestedEntries = collectVariables(newPrefix, config[key]);
            variables = new Map([...variables, ...nestedEntries]);
        } else {
            const variableName = `${prefix}${keyUppercase}`;
            const value = Array.isArray(config[key]) ? config[key].join(',') : config[key];

            variables.set(variableName, value);
        }
    }

    return variables;
}

async function loadAllConfigs(environment?: string) {
    if (!environment) {
        environment = await getEnvironment();
    }

    // Check that config folder exists (or initialize it).
    await checkConfigExistence(environment);

    // Accumulator to which we will load all the configs.
    let config = {};

    for (const configFile of CONFIG_FILES) {
        const localConfig = await loadConfig(environment, configFile);

        // Extend the `config` with the new values.
        deepExtend(config, localConfig);
    }

    return config;
}

export async function printAllConfigs(environment?: string) {
    const config = await loadAllConfigs(environment);
    console.log(`${JSON.stringify(config, null, 2)}`);
}

export async function compileConfig(environment?: string) {
    if (!environment) {
        environment = await getEnvironment();
    }

    const config = await loadAllConfigs(environment);

    const variables = collectVariables('', config);

    let outputFileContents = `# This file is generated automatically by 'zk config compile'\n`;
    outputFileContents += `# Do not edit manually!\n\n`;
    variables.forEach((value: string, key: string) => {
        outputFileContents += `${key}=${value}\n`;
    });

    const outputFileName = path.join(envDirPath(), `${environment}.env`);
    await fs.promises.writeFile(outputFileName, outputFileContents);
    console.log('Configs compiled');
}

export const command = new Command('config').description('config management');

command.command('load [environment]').description('load the config for a certain environment').action(printAllConfigs);
command
    .command('compile [environment]')
    .description('compile the config for a certain environment')
    .action(compileConfig);
