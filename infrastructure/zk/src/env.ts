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
export function modify(variable: string, assignedVariable: string) {
    const initEnv = 'etc/env/.init.env';
    if (!fs.existsSync(initEnv)) {
        fs.writeFileSync(initEnv, assignedVariable);
        return;
    }

    let source = fs.readFileSync(initEnv).toString();
    if (source.includes(variable)) {
        utils.replaceInFile(initEnv, `${variable}=.*`, assignedVariable.trim());
    } else {
        source += `\n${assignedVariable}`;
        fs.writeFileSync(initEnv, source);
    }

    reload();
}

export function removeFromInit(variable: string) {
    const initEnv = 'etc/env/.init.env';
    if (!fs.existsSync(initEnv)) {
        return;
    }

    utils.replaceInFile(initEnv, `${variable}=.*`, '');
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

export const command = new Command('env')
    .arguments('[env_name]')
    .description('get or set zksync environment')
    .action((envName?: string) => {
        envName ? set(envName, true) : get(true);
    });
