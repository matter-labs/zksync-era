import * as path from 'path';
import * as fs from 'fs';
import * as yaml from 'yaml';

export function shouldLoadConfigFromFile() {
    const chain = process.env.CHAIN_NAME;
    if (chain) {
        return {
            loadFromFile: true,
            chain
        } as const;
    } else {
        return {
            loadFromFile: false
        } as const;
    }
}

export const configNames = [
    'contracts.yaml',
    'general.yaml',
    'genesis.yaml',
    'secrets.yaml',
    'wallets.yaml',
    'external_node.yaml'
] as const;

export type ConfigName = (typeof configNames)[number];

export function loadEcosystem(pathToHome: string) {
    const configPath = path.join(pathToHome, '/ZkStack.yaml');
    if (!fs.existsSync(configPath)) {
        return [];
    }
    return yaml.parse(
        fs.readFileSync(configPath, {
            encoding: 'utf-8'
        })
    );
}

export function loadConfig({
    pathToHome,
    chain,
    configsFolder,
    configsFolderSuffix,
    config
}: {
    pathToHome: string;
    chain: string;
    configsFolder?: string;
    configsFolderSuffix?: string;
    config: ConfigName;
}) {
    const configPath = path.join(
        getConfigsFolderPath({ pathToHome, chain, configsFolder, configsFolderSuffix }),
        config
    );
    if (!fs.existsSync(configPath)) {
        return [];
    }
    return yaml.parse(
        fs.readFileSync(configPath, {
            encoding: 'utf-8'
        }),
        {
            customTags: (tags) =>
                tags.filter((tag) => {
                    if (typeof tag === 'string') {
                        return true;
                    }
                    if (tag.format !== 'HEX') {
                        return true;
                    }
                    return false;
                })
        }
    );
}

export function getConfigPath({
    pathToHome,
    chain,
    configsFolder,
    config
}: {
    pathToHome: string;
    chain: string;
    configsFolder?: string;
    config: ConfigName;
}) {
    return path.join(getConfigsFolderPath({ pathToHome, chain, configsFolder }), config);
}

export function getAllConfigsPath({
    pathToHome,
    chain,
    configsFolder
}: {
    pathToHome: string;
    chain: string;
    configsFolder?: string;
}) {
    const configPaths = {} as Record<ConfigName, string>;
    configNames.forEach((config) => {
        configPaths[config] = getConfigPath({ pathToHome, chain, configsFolder, config });
    });
    return configPaths;
}

export function getConfigsFolderPath({
    pathToHome,
    chain,
    configsFolder,
    configsFolderSuffix
}: {
    pathToHome: string;
    chain: string;
    configsFolder?: string;
    configsFolderSuffix?: string;
}) {
    return path.join(pathToHome, 'chains', chain, configsFolder ?? 'configs', configsFolderSuffix ?? '');
}
