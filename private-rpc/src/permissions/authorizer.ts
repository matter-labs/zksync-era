import { Address, Hex } from 'viem';
import { AccessDeniedRule, AccessRule } from '@/permissions/access-rules';
import YAML from 'yaml';
import { YamlParser } from '@/permissions/yaml-parser';
import { extractSelector } from '@/rpc/methods';
import { ResponseFilter } from '@/permissions/filter-response';

export class Authorizer {
    permissions: Map<string, AccessRule>;
    postReadFilters: Map<string, ResponseFilter>;

    constructor() {
        this.permissions = new Map();
        this.postReadFilters = new Map();
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

    static fromBuffer(buf: Buffer): Authorizer {
        const raw = YAML.parse(buf.toString());
        return new YamlParser(raw).parse();
    }
}
