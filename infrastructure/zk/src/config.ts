import { Command } from 'commander';
import * as toml from '@iarna/toml';
import * as fs from 'fs';
import deepExtend from 'deep-extend';

function loadConfigFile(path: string) {
    const fileContents = fs.readFileSync(path);
    try {
        return toml.parse(fileContents.toString());
    } catch (e: any) {
        console.error(`${path} load failed: Parsing error on line ${e.line} column ${e.column}: ${e.message}`);
        process.exit(1);
    }
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

export function loadConfig(env?: string): object {
    env ??= process.env.ZKSYNC_ENV!;

    let configPath = `${process.env.ZKSYNC_HOME}/etc/env/${env}.toml`;
    const config = {};
    if (fs.existsSync(configPath)) {
        loadConfigRecursive(config, configPath, []);
    } else {
        configPath = `${process.env.ZKSYNC_HOME}/etc/env/dev.toml`;
        loadConfigRecursive(config, configPath, []);
    }
    return config;
}

function loadConfigRecursive(config: object, configPath: string, calledFrom: string[]) {
    if (calledFrom.includes(configPath)) {
        throw new Error(`Config ${configPath} tried to include itself recursively from ${JSON.stringify(calledFrom)}`);
    }

    const overrides = loadConfigFile(configPath);
    if (overrides._metadata) {
        const metadata = overrides._metadata;
        delete overrides._metadata;

        if (typeof metadata !== 'object' || metadata instanceof Date || Array.isArray(metadata)) {
            throw new TypeError('Expected `_metadata` to be a table');
        }
        const basePaths = metadata.base || [];
        if (!Array.isArray(basePaths)) {
            throw new TypeError('Expected `_metadata.base` to be an array');
        }

        for (const basePath of basePaths) {
            if (typeof basePath !== 'string') {
                throw new TypeError('`_metadata.base` array entries must be strings (paths to base configs)');
            }
            const fullPath = `${process.env.ZKSYNC_HOME}/etc/env/${basePath}`;
            loadConfigRecursive(config, fullPath, [...calledFrom, configPath]);
        }
    }
    deepExtend(config, overrides);
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
    console.log(`Configs compiled for ${environment}`);
}

export const command = new Command('config').description('config management');

command.command('load [environment]').description('load the config for a certain environment').action(printAllConfigs);
command
    .command('compile [environment]')
    .description('compile the config for a certain environment')
    .action(compileConfig);
