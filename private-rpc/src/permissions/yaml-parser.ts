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

const publicSchema = z.object({ type: z.literal('public') });
const closedSchema = z.object({ type: z.literal('closed') });
const groupSchema = z.object({
    type: z.literal('group'),
    groups: z.array(z.string())
});
const checkArgumentSchema = z.object({
    type: z.literal('checkArgument'),
    argIndex: z.number()
});
const oneOfSchema = z.object({
    type: z.literal('oneOf'),
    rules: z.array(z.union([publicSchema, groupSchema, checkArgumentSchema]))
});

const ruleSchema = z.union([publicSchema, closedSchema, groupSchema, checkArgumentSchema, oneOfSchema]);
type Rule = z.infer<typeof ruleSchema>;

const methodSchema = z.object({
    signature: z.string(),
    read: ruleSchema,
    postRead: z.optional(z.object({ type: z.literal('responseIsCurrentUser'), index: z.number() })),
    write: ruleSchema
});
type RawMethod = z.infer<typeof methodSchema>;

const yamlSchema = z.object({
    groups: z.array(
        z.object({
            name: z.string(),
            members: z.array(addressSchema)
        })
    ),

    contracts: z.array(
        z.object({
            address: addressSchema,
            methods: z.array(methodSchema)
        })
    )
});

export class YamlParser {
    private yaml: z.infer<typeof yamlSchema>;
    private groups: Group[];

    constructor(yaml: unknown) {
        this.yaml = yamlSchema.parse(yaml);
        this.groups = this.yaml.groups.map(({ name, members }) => new Group(name, members));
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
                const members = rule.groups.map((name) => this.membersForGroup(name)).flat();
                return new GroupRule(members);
            case 'checkArgument':
                const [fnDef] = parseAbi([fnSignature]) as Abi;
                return new ArgumentIsCaller(rule.argIndex, fnDef as AbiFunction);
            case 'oneOf':
                const rules = rule.rules.map((r) => this.hidrateRule(r, fnSignature));
                return new OneOfRule(rules);
            default:
                throw new Error('Unknown rule type');
        }
    }

    parse(): Authorizer {
        const authorizer = new Authorizer();
        this.yaml.contracts.forEach((rawContract) => {
            const readPermissions = rawContract.methods.map((method) => {
                const selector = this.extractSelector(method);
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
