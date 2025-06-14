import { AbiFunction, Address, decodeAbiParameters, Hex, padHex } from 'viem';
import { areHexEqual } from '@/rpc/methods';
import { hexSchema } from '@/schemas/hex';

export interface ResponseFilter {
    canRead(user: Address, response: Hex): boolean;
}

export class ResponseIsCaller implements ResponseFilter {
    private index: number;
    private abi: AbiFunction;

    constructor(abi: AbiFunction, index: number) {
        this.abi = abi;
        this.index = index;
    }

    canRead(user: Address, response: Hex): boolean {
        const res = decodeAbiParameters(this.abi.outputs, response);
        const raw = res[this.index];
        const parsed = hexSchema.safeParse(raw);

        if (parsed.error) {
            return false;
        }

        return areHexEqual(padHex(user, { size: 32 }), padHex(parsed.data, { size: 32 }));
    }
}
