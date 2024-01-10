import { Command } from 'commander';
import fs from 'fs';
import dotenv from 'dotenv';
import * as utils from './utils';
import * as config from './config';

export const getAvailableEnvsFromFiles = () => {
    const envs = new Set();

    fs.readdirSync(`etc/env`).forEach((file) => {
        if (!file.startsWith('.') && (file.endsWith('.env') || file.endsWith('.toml'))) {
            envs.add(file.replace(/\..*$/, ''));
        }
    });
    return envs;
};

export function get(print: boolean = false) {
    const current = `etc/env/.current`;
    const inCurrent = fs.existsSync(current) && fs.readFileSync(current).toString().trim();

    const currentEnv = (process.env.ZKSYNC_ENV =
        process.env.ZKSYNC_ENV || inCurrent || (process.env.IN_DOCKER ? 'docker' : 'dev'));

    const envs = getAvailableEnvsFromFiles();

    if (print) {
        [...envs].sort().forEach((env) => {
            if (env === currentEnv) {
                console.log(`* ${env}`);
            } else {
                console.log(`  ${env}`);
            }
        });
    }

    return currentEnv;
}

export async function gitHooks() {
    if (fs.existsSync('.git')) {
        await utils.exec(`
            git config --local core.hooksPath ||
            git config --local core.hooksPath ${process.env.ZKSYNC_HOME}/.githooks
        `);
    }
}

export function set(env: string, print: boolean = false) {
    if (!fs.existsSync(`etc/env/${env}.env`) && !fs.existsSync(`etc/env/${env}.toml`)) {
        console.error(
            `Unknown environment: ${env}.\nCreate an environment file etc/env/${env}.env or etc/env/${env}.toml`
        );
        process.exit(1);
    }
    fs.writeFileSync('etc/env/.current', env);
    process.env.ZKSYNC_ENV = env;
    const envFile = (process.env.ENV_FILE = `etc/env/${env}.env`);
    if (!fs.existsSync(envFile)) {
        // No .env file found - we should compile it!
        config.compileConfig(env);
    }
    reload();
    get(print);
}

// override env with variables from init log
function loadInit() {
    if (fs.existsSync('etc/env/.init.env')) {
        const initEnv = dotenv.parse(fs.readFileSync('etc/env/.init.env'));
        for (const envVar in initEnv) {
            process.env[envVar] = initEnv[envVar];
        }
    }
}

// Unset env variables loaded from `.init.env` file
export function unloadInit() {
    if (fs.existsSync('etc/env/.init.env')) {
        const initEnv = dotenv.parse(fs.readFileSync('etc/env/.init.env'));
        for (const envVar in initEnv) {
            delete process.env[envVar];
        }
    }
}

// we have to manually override the environment
// because dotenv won't override variables that are already set
export function reload() {
    const env = dotenv.parse(fs.readFileSync(process.env.ENV_FILE!));
    for (const envVar in env) {
        process.env[envVar] = env[envVar];
    }
    loadInit();
}

// loads environment variables
export function load() {
    const zksyncEnv = get();
    const envFile = (process.env.ENV_FILE = `etc/env/${zksyncEnv}.env`);
    if (!fs.existsSync(envFile)) {
        // No .env file found - we should compile it!
        config.compileConfig();
    }
    dotenv.config({ path: envFile });
    loadInit();

    // This suppresses the warning that looks like: "Warning: Accessing non-existent property 'INVALID_ALT_NUMBER'...".
    // This warning is spawned from the `antlr4`, which is a dep of old `solidity-parser` library.
    // Old version of `solidity-parser` is still widely used, and currently we can't get rid of it fully.
    process.env.NODE_OPTIONS = '--no-warnings';
}

// places the environment logged by `zk init` variables into the .init.env file
export function modify(variable: string, assignedVariable: string, envFile: string = 'etc/env/.init.env') {
    if (!fs.existsSync(envFile)) {
        fs.writeFileSync(envFile, assignedVariable);
        return;
    }

    let source = fs.readFileSync(envFile).toString();
    if (source.includes(variable)) {
        utils.replaceInFile(envFile, `${variable}=.*`, assignedVariable.trim());
    } else {
        source += `\n${assignedVariable}`;
        fs.writeFileSync(envFile, source);
    }

    reload();
}

// merges .init.env with current env file so all configs are in the same place
export function mergeInitToEnv() {
    const env = dotenv.parse(fs.readFileSync(process.env.ENV_FILE!));
    const initEnv = dotenv.parse(fs.readFileSync('etc/env/.init.env'));
    for (const initVar in initEnv) {
        env[initVar] = initEnv[initVar];
    }
    let output = '';
    for (const envVar in env) {
        output += `${envVar}=${env[envVar]}\n`;
    }
    fs.writeFileSync(process.env.ENV_FILE!, output);
}

export function getAvailableEthTransferEvents() {
    return ['disabled', 'enabled'];
}

export function checkEthTransferEvents() {
    const EthTransferEventsEn = process.env.EN_API_ETH_TRANSFER_EVENTS;
    const EthTransferEventsServer = process.env.API_WEB3_JSON_RPC_API_ETH_TRANSFER_EVENTS;

    if (EthTransferEventsEn && EthTransferEventsServer && EthTransferEventsEn !== EthTransferEventsServer) {
        console.error(
            `Api mode mismatch: ${EthTransferEventsEn} != ${EthTransferEventsServer}.\nPlease, check your .env files.`
        );
        process.exit(1);
    }
}

export function getEthTransferEvents(print: boolean) {
    checkEthTransferEvents();

    const EthTransferEvents =
        process.env.EN_API_ETH_TRANSFER_EVENTS ?? process.env.API_WEB3_JSON_RPC_API_ETH_TRANSFER_EVENTS;

    if (print) {
        const modes = getAvailableEthTransferEvents();

        if (!EthTransferEvents || !modes.includes(EthTransferEvents)) {
            console.error('Unknown api mode or api mode is not set.\nPlease, check your .env files.');
            process.exit(1);
        }

        for (const mode of modes) {
            if (mode === EthTransferEvents) {
                console.log(`* ${mode}`);
            } else {
                console.log(`  ${mode}`);
            }
        }
    }

    return EthTransferEvents;
}

export function setEthTransferEvents(mode: string, print: boolean) {
    checkEthTransferEvents();

    const modes = getAvailableEthTransferEvents();
    if (!modes.includes(mode)) {
        console.error(`Unknown api mode: ${mode}.\nPlease, check your .env files.`);
        process.exit(1);
    }

    if (fs.existsSync('etc/env/ext-node.env')) {
        modify('EN_API_ETH_TRANSFER_EVENTS', `EN_API_ETH_TRANSFER_EVENTS=${mode}`, 'etc/env/ext-node.env');
    }
    if (fs.existsSync('etc/env/ext-node-docker.env')) {
        modify('EN_API_ETH_TRANSFER_EVENTS', `EN_API_ETH_TRANSFER_EVENTS=${mode}`, 'etc/env/ext-node-docker.env');
    }
    if (fs.existsSync('etc/env/dev.env')) {
        modify(
            'API_WEB3_JSON_RPC_API_ETH_TRANSFER_EVENTS',
            `API_WEB3_JSON_RPC_API_ETH_TRANSFER_EVENTS=${mode}`,
            'etc/env/dev.env'
        );
    }

    getEthTransferEvents(print);
}

export const command = new Command('env')
    .arguments('[env_name]')
    .description('get or set zksync environment')
    .action((envName?: string) => {
        envName ? set(envName, true) : get(true);
    });

command
    .command('eth-transfer-events')
    .arguments('[mode]')
    .description('get or set mode for ETH transfer events')
    .action((mode?: string) => {
        mode ? setEthTransferEvents(mode, true) : getEthTransferEvents(true);
    });
