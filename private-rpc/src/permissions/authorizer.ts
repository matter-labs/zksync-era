import { Address, Hex } from 'viem';
import { AccessDeniedRule, AccessRule } from '@/permissions/access-rules';
import { YamlParser } from '@/permissions/yaml-parser';
import { extractSelector } from '@/rpc/methods';
import { ResponseFilter } from '@/permissions/filter-response';
import { env } from '@/env';

export class Authorizer {
    permissions: Map<string, AccessRule>;
    postReadFilters: Map<string, ResponseFilter>;
    whitelistedWallets: Set<Address> | 'all';

    constructor() {
        this.permissions = new Map();
        this.postReadFilters = new Map();
        this.whitelistedWallets = new Set();
    }

    addReadRule(address: Address, method: Hex, rule: AccessRule): void {
        const key = `read_contract:${address}:${method}`;
        this.permissions.set(key, rule);
    }

    addWriteRule(address: Address, method: Hex, rule: AccessRule): void {
        const key = `write_contract:${address}:${method}`;
        this.permissions.set(key, rule);
    }

    addPostReadFilter(address: Address, method: Hex, rule: ResponseFilter): void {
        this.postReadFilters.set(`${address}:${method}`, rule);
    }

    checkContractRead(address: Address, data: Hex, user: Address) {
        const method = extractSelector(data);
        const rule = this.permissions.get(`read_contract:${address}:${method}`) || new AccessDeniedRule();
        return rule.canDo(user, data);
    }

    checkContractWrite(address: Address, data: Hex, user: Address) {
        const method = extractSelector(data);
        const rule = this.permissions.get(`write_contract:${address}:${method}`) || new AccessDeniedRule();
        return rule.canDo(user, data);
    }

    checkPostReadFilter(address: Address, data: Hex): ResponseFilter | null {
        const method = extractSelector(data);
        return this.postReadFilters.get(`${address}:${method}`) || null;
    }

    isAddressWhitelisted(address: Address): boolean {
        if (this.whitelistedWallets === 'all') {
            return true;
        }
        return this.whitelistedWallets.has(address);
    }

    reloadFromEnv(): Authorizer {
        const filePath = env.PERMISSIONS_YAML_PATH;
        console.log(`loading permissions from ${filePath}`);
        this.permissions = new Map();
        this.postReadFilters = new Map();
        const parser = new YamlParser(filePath);
        parser.load_rules(this);

        const wallets = parser.getWhitelistedWallets();
        if (wallets === 'all') {
            this.whitelistedWallets = 'all';
        } else {
            this.whitelistedWallets = new Set(wallets);
        }
        return this;
    }
}
