import { findHome } from './zksync-home';
import { loadConfig } from 'utils/build/file-configs';

export function getMainWalletPk(chainName: string): string {
    const walletsConfig = loadConfig({
        pathToHome: findHome(),
        chain: chainName,
        config: 'wallets.yaml'
    });
    const pk = walletsConfig?.test_wallet?.private_key;
    if (typeof pk !== 'string') {
        throw new Error(
            `test_wallet.private_key not found in wallets.yaml ` +
                `Run "zkstack dev init-test-wallet --chain ${chainName}" first.`
        );
    }
    return pk;
}
