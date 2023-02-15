import * as fs from 'fs';

function configPath(postfix: string) {
    return `${process.env.ZKSYNC_HOME}/etc/test_config/${postfix}`;
}

function loadConfig(path: string) {
    return JSON.parse(
        fs.readFileSync(path, {
            encoding: 'utf-8'
        })
    );
}

export function loadTestConfig(withWithdrawalHelpers: boolean) {
    const ethConstantPath = configPath('constant/eth.json');
    const ethConfig = loadConfig(ethConstantPath);

    if (withWithdrawalHelpers) {
        const withdrawalHelpersConfigPath = configPath('volatile/withdrawal-helpers.json');
        const withdrawalHelpersConfig = loadConfig(withdrawalHelpersConfigPath);
        return {
            eth: ethConfig,
            withdrawalHelpers: withdrawalHelpersConfig
        };
    } else {
        return {
            eth: ethConfig
        };
    }
}

export type Token = {
    name: string;
    symbol: string;
    decimals: number;
    address: string;
};

export function getTokens(network: string): Token[] {
    const configPath = `${process.env.ZKSYNC_HOME}/etc/tokens/${network}.json`;
    return JSON.parse(
        fs.readFileSync(configPath, {
            encoding: 'utf-8'
        })
    );
}
