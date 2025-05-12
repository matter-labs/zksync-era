import { Address } from 'viem';

export class Group {
    name: string;
    members: Address[];

    constructor(name: string, members: Address[]) {
        this.name = name;
        this.members = members;
    }
}
