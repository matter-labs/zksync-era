import { z } from 'zod';
import { Group } from '@/permissions/group';
import { Authorizer } from '@/permissions/authorizer';
import { addressSchema } from '@/schemas/address';
import { Abi, AbiFunction, Address, Hex, parseAbi, toFunctionSelector } from 'viem';
import {
    AccessDeniedRule,
    AccessRule,
    ArgumentIsCaller,
    GroupRule,
    OneOfRule,
    PublicRule
} from '@/permissions/access-rules';
import { Permission } from '@/permissions/permission';
import { ResponseIsCaller } from '@/permissions/filter-response';
import { readFileSync } from 'node:fs';
import YAML from 'yaml';

const publicSchema = z.object({ type: z.literal('public') });
const closedSchema = z.object({ type: z.literal('closed') });
const groupSchemaRaw = z.object({
    type: z.literal('group'),
    groups: z.array(z.string())
});
const checkArgumentSchema = z.object({
    type: z.literal('checkArgument'),
    argIndex: z.number()
});
const oneOfSchema = z.object({
    type: z.literal('oneOf'),
    rules: z.array(z.union([publicSchema, groupSchemaRaw, checkArgumentSchema]))
});

const ruleSchema = z.union([publicSchema, closedSchema, groupSchemaRaw, checkArgumentSchema, oneOfSchema]);
type Rule = z.infer<typeof ruleSchema>;

const methodSchema = z.object({
    signature: z.string(),
    read: ruleSchema,
    postRead: z.optional(z.object({ type: z.literal('responseIsCurrentUser'), index: z.number() })),
    write: ruleSchema
});
type RawMethod = z.infer<typeof methodSchema>;

const groupSchema = z.object({
            name: z.string(),
            members: z.array(addressSchema)
});
type RawGroup = z.infer<typeof groupSchema>;

const contractSchema = z.object({
            address: addressSchema,
            methods: z.array(methodSchema)
});

type RawContract = z.infer<typeof contractSchema>;

const yamlSchema = z.object({
    whitelisted_wallets: z
        .array(z.union([addressSchema, z.literal('all')]))
        .nonempty({ message: 'whitelisted_wallets cannot be empty. To allow all wallets, use ["all"].' }),
    groups: z.array(groupSchema),
    contracts: z.array(contractSchema)
});

export class YamlParser {
    private yaml: z.infer<typeof yamlSchema>;
    private groups: Group[];

    constructor(filePath: string) {
        const buf = readFileSync(filePath);
        const raw = YAML.parse(buf.toString());
        this.yaml = yamlSchema.parse(raw);
        this.groups = this.yaml.groups.map(({ name, members }: RawGroup) => new Group(name, members));
    }

    getWhitelistedWallets(): (Address | 'all')[] {
        return this.yaml.whitelisted_wallets;
    }

    private extractSelector(method: RawMethod): Hex {
        return toFunctionSelector(method.signature);
    }

    private membersForGroup(groupName: string): Address[] {
        return (
            this.groups
                .filter((g) => g.name === groupName)
                .map((g) => g.members)
                .flat()
                .map((a) => addressSchema.parse(a)) || ([] as Address[])
        );
    }

    private hidrateRule(rule: Rule, fnSignature: string): AccessRule {
        switch (rule.type) {
            case 'public':
                return new PublicRule();
            case 'closed':
                return new AccessDeniedRule();
            case 'group':
                const members = rule.groups.map((name: string) => this.membersForGroup(name)).flat();
                return new GroupRule(members);
            case 'checkArgument':
                const [fnDef] = parseAbi([fnSignature]) as Abi;
                return new ArgumentIsCaller(rule.argIndex, fnDef as AbiFunction);
            case 'oneOf':
                const rules = rule.rules.map((r: Rule) => this.hidrateRule(r, fnSignature));
                return new OneOfRule(rules);
            default:
                throw new Error('Unknown rule type');
        }
    }

    load_rules(authorizer: Authorizer) {
        this.yaml.contracts.forEach((rawContract: RawContract) => {
            const readPermissions = rawContract.methods.map((method: RawMethod) => {
                const selector = method.signature === '#BASE_TOKEN_TRANSFER' ? '0x' : this.extractSelector(method);

                const readRule = this.hidrateRule(method.read, method.signature);
                const writeRule = this.hidrateRule(method.write, method.signature);
                authorizer.addReadRule(rawContract.address, selector, readRule);
                authorizer.addWriteRule(rawContract.address, selector, writeRule);

                if (method.postRead) {
                    const [fnDef] = parseAbi([method.signature]) as Abi;
                    const filter = new ResponseIsCaller(fnDef as AbiFunction, method.postRead.index);
                    authorizer.addPostReadFilter(rawContract.address, selector, filter);
                }

                return [
                    Permission.contractRead(rawContract.address, selector, readRule),
                    Permission.contractWrite(rawContract.address, selector, writeRule)
                ];
            });

            return readPermissions.flat();
        });

        return authorizer;
    }
}
