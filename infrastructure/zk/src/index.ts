#!/usr/bin/env node

import { program, Command } from 'commander';
import { spawnSync } from 'child_process';
import { serverCommand as server, enCommand as en } from './server';
import { command as contractVerifier } from './contract_verifier';
import { command as up } from './up';
import { command as down } from './down';
import { command as contract } from './contract';
import { initCommand as init, reinitCommand as reinit, lightweightInitCommand as lightweightInit } from './init';
import { initHyperchainCommand as initHyperchain } from './hyperchain_wizard';
import { command as run } from './run';
import { command as test } from './test/test';
import { command as docker } from './docker';
import { command as fmt } from './fmt';
import { command as lint } from './lint';
import { command as compiler } from './compiler';
import { command as completion } from './completion';
import { command as config } from './config';
import { command as clean } from './clean';
import { command as db } from './database';
import { command as verifyUpgrade } from './verify-upgrade';
import { proverCommand } from './prover_setup';
import { command as status } from './status';
import { command as spellcheck } from './spellcheck';
import { command as linkcheck } from './linkcheck';
import { command as setupEn } from './setup_en';
import * as env from './env';

const COMMANDS = [
    server,
    en,
    contractVerifier,
    up,
    down,
    db,
    contract,
    init,
    reinit,
    lightweightInit,
    initHyperchain,
    run,
    test,
    fmt,
    lint,
    docker,
    config,
    clean,
    compiler,
    verifyUpgrade,
    proverCommand,
    env.command,
    status,
    spellcheck,
    linkcheck,
    setupEn,
    completion(program as Command)
];

async function main() {
    const cwd = process.cwd();
    const ZKSYNC_HOME = process.env.ZKSYNC_HOME;

    if (!ZKSYNC_HOME) {
        throw new Error('Please set $ZKSYNC_HOME to the root of zkSync repo!');
    } else {
        process.chdir(ZKSYNC_HOME);
    }

    env.load();

    program.version('0.1.0').name('zk').description('zksync workflow tools');

    for (const command of COMMANDS) {
        program.addCommand(command);
    }

    // f command is special-cased because it is necessary
    // for it to run from $PWD and not from $ZKSYNC_HOME
    program
        .command('f <command...>')
        .allowUnknownOption()
        .action((command: string[]) => {
            process.chdir(cwd);
            const result = spawnSync(command[0], command.slice(1), { stdio: 'inherit' });
            if (result.error) {
                throw result.error;
            }
            process.exitCode = result.status || undefined;
        });

    await program.parseAsync(process.argv);
}

main().catch((err: Error) => {
    console.error('Error:', err.message || err);
    process.exitCode = 1;
});
