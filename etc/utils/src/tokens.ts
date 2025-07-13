import * as path from 'path';
import * as fs from 'fs';
import * as yaml from 'yaml';
import fsSync from 'fs';

import * as zksync from 'zksync-ethers';

interface TokensDict {
    [key: string]: L1Token;
}

type Tokens = {
    tokens: TokensDict;
};

export type L1Token = {
    name: string;
    symbol: string;
    decimals: bigint;
    address: string;
};

export function getToken(pathToHome: string, baseTokenAddress: zksync.types.Address): { token: L1Token, baseToken: L1Token | undefined } {
    const tokens = getTokensNew(pathToHome);
    // wBTC is chosen because it has decimals different from ETH (8 instead of 18).
    // Using this token will help us to detect decimals-related errors.
    // but if it's not available, we'll use the first token from the list.
    let token = tokens.tokens['WBTC'];
    if (token === undefined) {
        token = Object.values(tokens.tokens)[0];
        if (token.symbol == 'WETH') {
            token = Object.values(tokens.tokens)[1];
        }
    }
    let baseToken;

    for (const key in tokens.tokens) {
        const token = tokens.tokens[key];
        if (zksync.utils.isAddressEq(token.address, baseTokenAddress)) {
            baseToken = token;
        }
    }
    return { token, baseToken };
}

function getTokensNew(pathToHome: string): Tokens {
    const configPath = path.join(pathToHome, '/configs/erc20.yaml');
    if (!fs.existsSync(configPath)) {
        throw Error('Tokens config not found');
    }

    const parsedObject = yaml.parse(
        fs.readFileSync(configPath, {
            encoding: 'utf-8'
        }),
        {
            customTags
        }
    );

    for (const key in parsedObject.tokens) {
        parsedObject.tokens[key].decimals = BigInt(parsedObject.tokens[key].decimals);
    }
    return parsedObject;
}

function customTags(tags: yaml.Tags): yaml.Tags {
    for (const tag of tags) {
        // @ts-ignore
        if (tag.format === 'HEX') {
            // @ts-ignore
            tag.resolve = (str, _onError, _opt) => {
                return str;
            };
        }
    }
    return tags;
}