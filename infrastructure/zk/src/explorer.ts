import { Command } from 'commander';
import * as utils from './utils';


export const explorerCommand = new Command('explorer')
    .description('start block explorer')
    .action(async (cmd: Command) => {
        await utils.spawn("npm install --prefix block-explorer/")
        await utils.spawn("npm run hyperchain:configure --prefix block-explorer/")
        await utils.spawn("npm run db:create --prefix block-explorer/")
        await utils.spawn("npm run dev --prefix block-explorer/")
    });