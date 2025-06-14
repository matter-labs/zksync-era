import { AbiFunction, Address, decodeFunctionData, Hex, isAddressEqual } from 'viem';
import { addressSchema } from '@/schemas/address';

export interface AccessRule {
    canDo(user: Address, calldata: Hex): boolean;
}

export class PublicRule implements AccessRule {
    canDo(_address: Address): boolean {
        return true;
    }
}

export class AccessDeniedRule implements AccessRule {
    canDo(_address: Address): boolean {
        return false;
    }
}

export class GroupRule implements AccessRule {
    members: Set<Address>;

    constructor(members: Address[]) {
        this.members = new Set(members);
    }

    canDo(address: Address): boolean {
        return this.members.has(address);
    }
}

export class OneOfRule implements AccessRule {
    rules: AccessRule[];

    constructor(rules: AccessRule[]) {
        this.rules = rules;
    }

    canDo(user: Address, calldata: Hex): boolean {
        return this.rules.some((rule) => rule.canDo(user, calldata));
    }
}

export class ArgumentIsCaller implements AccessRule {
    argIndex: number;
    functionDef: AbiFunction;

    constructor(argIndex: number, functionDef: AbiFunction) {
        this.argIndex = argIndex;
        this.functionDef = functionDef;
    }

    canDo(user: Address, callData: Hex): boolean {
        const { args } = decodeFunctionData({
            abi: [this.functionDef],
            data: callData
        });

        const arg = addressSchema.parse(args[this.argIndex]);
        return isAddressEqual(arg, user);
    }
}
