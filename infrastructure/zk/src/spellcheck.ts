import { Command } from 'commander';
import * as utils from 'utils';

export async function runSpellCheck(pattern: string, useCargo: boolean, useCSpell: boolean) {
    // Default commands for cSpell and cargo spellcheck
    const cSpellCommand = `cspell "${pattern}" --config=./checks-config/cspell.json`;
    const cargoCommand = `cargo spellcheck --cfg=./checks-config/era.cfg --code 1`;
    // Necessary to run cargo spellcheck in the prover directory explicitly as
    // it is not included in the root cargo.toml file
    const cargoCommandForProver = `cargo spellcheck --cfg=../checks-config/era.cfg --code 1`;

    try {
        let results = [];

        // Run cspell over all **/*.md files
        if (useCSpell || (!useCargo && !useCSpell)) {
            results.push(await utils.spawn(cSpellCommand));
        }

        // Run cargo spellcheck in core and prover directories
        if (useCargo || (!useCargo && !useCSpell)) {
            results.push(await utils.spawn(cargoCommand));
            results.push(await utils.spawn('cd prover && ' + cargoCommandForProver));
        }

        // Check results and exit with error code if any command failed
        if (results.some((code) => code !== 0)) {
            console.error('Spell check failed');
            process.exit(1);
        }
    } catch (error) {
        console.error('Error occurred during spell checking:', error);
        process.exit(1);
    }
}

export const command = new Command('spellcheck')
    .option('--pattern <pattern>', 'Glob pattern for files to check', '**/*.md')
    .option('--use-cargo', 'Use cargo spellcheck')
    .option('--use-cspell', 'Use cspell')
    .description('Run spell check on specified files')
    .action((cmd) => {
        runSpellCheck(cmd.pattern, cmd.useCargo, cmd.useCSpell);
    });
