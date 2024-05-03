import { Command } from 'commander';
import * as utils from './utils';

export async function runMarkdownLinkCheck(configPath: string) {
    // Command line usage for markdown-link-check suggests using find and xargs for
    // recursive checks. See: `https://github.com/tcort/markdown-link-check?tab=readme-ov-file#check-links-from-a-local-markdown-folder-recursive`
    const findCommand = `find . -name "*.md" ! -path "*/node_modules/*" ! -path "*/target/release/*" ! -path "*/build/*" ! -path "*/contracts/*" -print0`;
    const markdownLinkCheckCommand = `xargs -0 -n1 markdown-link-check --config ${configPath}`;
    const fullCommand = `${findCommand} | ${markdownLinkCheckCommand}`;

    try {
        await utils.spawn(fullCommand);
        console.log('Markdown link check completed successfully');
    } catch (error) {
        console.error('Error occurred during markdown link checking:', error);
        process.exit(1);
    }
}

export const command = new Command('linkcheck')
    .option('--config <config>', 'Path to configuration file', './checks-config/links.json')
    .description('Run markdown link check on specified files')
    .action((cmd) => {
        runMarkdownLinkCheck(cmd.config);
    });
