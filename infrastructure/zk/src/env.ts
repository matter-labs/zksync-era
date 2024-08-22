import { Command } from 'commander';
import fs from 'fs';
import dotenv from 'dotenv';
import * as utils from 'utils';
import * as config from './config';

export const getAvailableEnvsFromFiles = () => {
    const envs = new Set();

    [...fs.readdirSync('etc/env/target'), ...fs.readdirSync('etc/env/configs')].forEach((file) => {
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

export function set(environment: string, print: boolean = false) {
    if (!fs.existsSync(`etc/env/target/${environment}.env`) && !fs.existsSync(`etc/env/configs/${environment}.toml`)) {
        console.error(
            `Unknown environment: ${environment}.\nCreate an environment file etc/env/target/${environment}.env or etc/env/configs/${environment}.toml`
        );
        process.exit(1);
    }
    fs.writeFileSync('etc/env/.current', environment);
    process.env.ZKSYNC_ENV = environment;
    const envFile = (process.env.ENV_FILE = `etc/env/target/${environment}.env`);
    if (!fs.existsSync(envFile)) {
        // No .env file found - we should compile it!
        config.compileConfig(environment);
    }
    reload();
    get(print);
}

// we have to manually override the environment
// because dotenv won't override variables that are already set
export function reload(environment?: string) {
    environment = environment ?? get();
    config.compileConfig();
    const envFile = (process.env.ENV_FILE = `etc/env/target/${environment}.env`);
    const env = dotenv.parse(fs.readFileSync(envFile));
    for (const envVar in env) {
        console.log(`Reloading: ${envVar}=${env[envVar]}`)
        process.env[envVar] = env[envVar];
    }
}

// loads environment variables
export function load() {
    fs.mkdirSync('etc/env/target', { recursive: true });
    const environment = get();
    const envFile = (process.env.ENV_FILE = `etc/env/target/${environment}.env`);
    if (!fs.existsSync(envFile)) {
        // No .env file found - we should compile it!
        config.compileConfig();
    }
    dotenv.config({ path: envFile });

    // This suppresses the warning that looks like: "Warning: Accessing non-existent property 'INVALID_ALT_NUMBER'...".
    // This warning is spawned from the `antlr4`, which is a dep of old `solidity-parser` library.
    // Old version of `solidity-parser` is still widely used, and currently we can't get rid of it fully.
    process.env.NODE_OPTIONS = '--no-warnings';
}

// places the environment logged by `zk init` variables into the .init.env file
export function modify(variable: string, value: string, initEnv: string, withReload = true) {
    console.log(`MODIFYING ENV VARIABLE ${variable} to ${value}`)
    console.log(`initEnv ${initEnv}`)
    const assignedVariable = value.startsWith(`${variable}=`) ? value : `${variable}=${value}`;
    fs.mkdirSync('etc/env/l2-inits', { recursive: true });
    fs.mkdirSync('etc/env/l1-inits', { recursive: true });
    if (!fs.existsSync(initEnv)) {
        fs.writeFileSync(initEnv, assignedVariable);
        return;
    }

    let source = fs.readFileSync(initEnv).toString();
    if (source.includes(variable)) {
        utils.replaceInFile(initEnv, `${variable}.*`, assignedVariable.trim());
    } else {
        source += `\n${assignedVariable}`;
        fs.writeFileSync(initEnv, source);
    }

    if (withReload) {
        reload();
    }
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
    const environment = process.env.ZKSYNC_ENV!;
    const env = dotenv.parse(fs.readFileSync(`etc/env/target/${environment}.env`));
    const initEnv = dotenv.parse(fs.readFileSync(`etc/env/l2-inits/${environment}.init.env`));
    for (const initVar in initEnv) {
        env[initVar] = initEnv[initVar];
    }
    let output = '';
    for (const envVar in env) {
        output += `${envVar}=${env[envVar]}\n`;
    }
    fs.writeFileSync(`etc/env/target/${environment}.env`, output);
}

export const command = new Command('env')
    .arguments('[env_name]')
    .description('get or set zksync environment')
    .action((environment?: string) => {
        environment ? set(environment, true) : get(true);
    });
