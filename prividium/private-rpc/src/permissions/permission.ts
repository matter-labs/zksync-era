import { Address, Hex } from 'viem';
import { AccessRule } from '@/permissions/access-rules';

export class Permission {
    key: string;
    rule: AccessRule;

    constructor(key: string, rule: AccessRule) {
        this.key = key;
        this.rule = rule;
    }

    static contractRead(addr: Address, method: Hex, rule: AccessRule): Permission {
        return new this(`read_contract:${addr}:${method}`, rule);
    }

    static contractWrite(addr: Address, method: Hex, rule: AccessRule): Permission {
        return new this(`write_contract:${addr}:${method}`, rule);
    }
}
