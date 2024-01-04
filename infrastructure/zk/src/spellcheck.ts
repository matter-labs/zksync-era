import { Command } from 'commander';
import * as utils from './utils';

export async function runSpellCheck(pattern: string, config: string, useCargo: boolean, useCSpell: boolean) {
    const cSpellCommand = `cspell ${pattern} --config=./spellcheck/cspell.json`;
    const cargoCommand = `cargo spellcheck --cfg=./spellcheck/era.cfg`;

    try {
        if (!useCargo && !useCSpell) {
            await Promise.all([utils.spawn(cSpellCommand), utils.spawn(cargoCommand)]);
        } else {
            // Run cspell if specified
            // zk spellcheck --use-cspell
            if (useCSpell) {
                await utils.spawn(cSpellCommand);
            }

            // Run cargo spellcheck if specified
            // zk spellcheck --use-cargo
            if (useCargo) {
                await utils.spawn(cargoCommand);
            }
        }
    } catch (error) {
        console.error('Error occurred during spell checking:', error);
    }
}

export const command = new Command('spellcheck')
    .option('--pattern <pattern>', 'Glob pattern for files to check', 'docs/**/*')
    .option('--config <config>', 'Path to configuration file', './spellcheck/cspell.json')
    .option('--use-cargo', 'Use cargo spellcheck')
    .option('--use-cspell', 'Use cspell')
    .description('Run spell check on specified files')
    .action((cmd) => {
        runSpellCheck(cmd.pattern, cmd.config, cmd.useCargo, cmd.useCSpell);
    });
