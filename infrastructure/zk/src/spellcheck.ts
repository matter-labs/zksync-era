import { Command } from 'commander';
import * as utils from './utils';

export async function runSpellCheck(pattern: string, useCargo: boolean, useCSpell: boolean) {
    const cSpellCommand = `cspell ${pattern} --config=./spellcheck/cspell.json`;
    const cargoCommand = `cargo spellcheck --cfg=./spellcheck/era.cfg`;

    try {
        let results = [];
        if (!useCargo && !useCSpell) {
            results = await Promise.all([utils.spawn(cSpellCommand), utils.spawn(cargoCommand)]);
        } else {
            // Run cspell if specified
            if (useCSpell) {
                results.push(await utils.spawn(cSpellCommand));
            }

            // Run cargo spellcheck if specified
            if (useCargo) {
                results.push(await utils.spawn(cargoCommand));
            }
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
    .option('--pattern <pattern>', 'Glob pattern for files to check', 'docs/**/*')
    .option('--config <config>', 'Path to configuration file', './spellcheck/cspell.json')
    .option('--use-cargo', 'Use cargo spellcheck')
    .option('--use-cspell', 'Use cspell')
    .description('Run spell check on specified files')
    .action((cmd) => {
        runSpellCheck(cmd.pattern, cmd.useCargo, cmd.useCSpell);
    });
