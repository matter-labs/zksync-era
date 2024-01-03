import { Command } from 'commander';
import * as toml from '@iarna/toml';
import * as fs from 'fs';
import deepExtend from 'deep-extend';

const CONFIG_FILES = [
    'alerts.toml',
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
    'rust.toml',
    'private.toml',
    'witness_generator.toml',
    'house_keeper.toml',
    'fri_prover.toml',
    'fri_witness_generator.toml',
    'fri_prover_group.toml',
    'proof_data_handler.toml',
    'fri_witness_vector_generator.toml',
    'fri_prover_gateway.toml',
    'fri_proof_compressor.toml'
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

export function loadConfig(env?: string, forceAll: boolean = false): any {
    env ??= process.env.ZKSYNC_ENV!;
    let config = {};

    if (forceAll || !env.startsWith('ext-node')) {
        for (const configFile of CONFIG_FILES) {
            const localConfig = loadConfigFile(`etc/env/base/${configFile}`);
            deepExtend(config, localConfig);
        }
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
    console.log(`Configs compiled for ${environment}`);
}

export const command = new Command('config').description('config management');

command.command('load [environment]').description('load the config for a certain environment').action(printAllConfigs);
command
    .command('compile [environment]')
    .description('compile the config for a certain environment')
    .action(compileConfig);
