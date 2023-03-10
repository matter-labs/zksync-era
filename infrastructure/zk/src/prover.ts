import { Command } from 'commander';
import * as utils from './utils';
import os from 'os';

export async function prover(totalProvers: number) {
    let children: Promise<unknown>[] = [];

    for (let id = 1; id <= totalProvers; id++) {
        const name = `${os.hostname()}_${id}_blocks`;
        console.log('Started prover', name);
        const child = utils.spawn(
            `cargo run --release --bin zksync_prover -- --worker_name=${name} plonk-step-by-step`
        );
        children.push(child);
    }

    await Promise.all(children);
}

export const command = new Command('prover')
    .description('run zksync prover')
    .arguments('[number_of_provers]')
    .action(async (provers?: string) => {
        const totalProvers = provers ? parseInt(provers) : 1;
        await prover(totalProvers);
    });
