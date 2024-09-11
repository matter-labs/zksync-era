import { program } from 'commander';

import { command as publish } from './l2upgrade/system-contracts';
import { command as manager } from './protocol-upgrade-manager';
import { command as customUpgrade } from './custom-upgrade';
import { command as l1Upgrade } from './l1upgrade/facets';
import { command as l2Upgrade } from './l2upgrade/transactions';
import { command as transactions } from './transaction';
import { command as crypto } from './crypto/crypto';
import { command as hyperchainUpgrade } from './hyperchain-upgrade';

const COMMANDS = [publish, manager, customUpgrade, l1Upgrade, transactions, crypto, l2Upgrade, hyperchainUpgrade];

async function main() {
    const ZKSYNC_HOME = process.env.ZKSYNC_HOME;

    if (!ZKSYNC_HOME) {
        throw new Error('Please set $ZKSYNC_HOME to the root of ZKsync repo!');
    } else {
        process.chdir(ZKSYNC_HOME);
    }

    program.version('0.1.0').name('zk').description('zksync protocol upgrade tools');

    for (const command of COMMANDS) {
        program.addCommand(command);
    }
    await program.parseAsync(process.argv);
}

main().catch((err: Error) => {
    console.error('Error:', err.message || err);
    process.exitCode = 1;
});
