import { Hex } from 'viem';
import { MethodHandler, RequestContext } from '@/rpc/rpc-service';
import { FastifyReplyType } from 'fastify/types/type-provider';
import { request, unauthorized } from '@/rpc/json-rpc';
import { delegateCall } from '@/rpc/delegate-call';
import { z, ZodTypeAny } from 'zod';
import { addressSchema } from '@/schemas/address';
import { env } from '@/env';
import { authorizer } from '@/permissions';
import { hexSchema } from '@/schemas/hex';
import { bigintStringSchema } from '@/schemas/numeric';
import { isAddressEqual } from 'viem';

export function extractSelector(calldata: Hex): Hex {
    return calldata.substring(0, 10) as Hex;
}

export function forbiddenMethod(name: string): MethodHandler {
    return {
        name,
        async handle(
            _context: RequestContext,
            _method: string,
            _params: unknown[],
            id: number | string
        ): Promise<FastifyReplyType> {
            return unauthorized(id);
        }
    };
}

export function unrestricted(name: string): MethodHandler {
    return {
        name,
        async handle(
            context: RequestContext,
            method: string,
            params: unknown[],
            id: number | string
        ): Promise<FastifyReplyType> {
            return delegateCall({ url: context.targetRpcUrl, id, method, params });
        }
    };
}

export function areHexEqual(a: Hex, b: Hex): boolean {
    return a.toLowerCase() === b.toLowerCase();
}

export async function sendToTargetRpc<T extends ZodTypeAny>(
    targetRpcUrl: string,
    id: number | string,
    method: string,
    params: unknown[],
    schema: T
): Promise<z.infer<T>> {
    return fetch(targetRpcUrl, {
        method: 'POST',
        body: JSON.stringify(request({ id, method, params })),
        headers: { 'Content-Type': 'application/json' }
    })
        .then((res) => res.json())
        .then((json) => schema.parse(json));
}

const onlyUserParamsSchema = (additional: ZodTypeAny[] = []) => z.tuple([addressSchema, ...additional]);

export function onlyCurrentUser(name: string, additionalParamsSchema: ZodTypeAny[] = []) {
    return {
        name: name,
        async handle(
            context: RequestContext,
            method: string,
            params: unknown[],
            id: number | string
        ): Promise<FastifyReplyType> {
            const user = context.currentUser;
            const [target] = onlyUserParamsSchema(additionalParamsSchema).parse(params);
            if (user !== target) {
                return unauthorized(id);
            }

            return delegateCall({ url: context.targetRpcUrl, id, method, params });
        }
    };
}

const callReqSchema = z
    .object({
        from: addressSchema.optional(),
        to: addressSchema,
        gas: hexSchema.optional(),
        gas_price: hexSchema.optional(),
        max_fee_per_gas: hexSchema.optional(),
        max_priority_fee_per_gas: hexSchema.optional(),
        value: hexSchema.optional(),
        data: hexSchema.optional(),
        input: hexSchema.optional(),
        nonce: hexSchema.optional(),
        transaction_type: hexSchema.optional(),
        access_list: z.array(z.tuple([addressSchema, z.array(hexSchema)])).optional(),
        customData: z
            .object({
                gasPerPubdata: bigintStringSchema,
                customSignature: hexSchema,
                paymasterParams: z.object({
                    paymaster: addressSchema,
                    paymasterInput: hexSchema
                }),
                factoryDeps: z.array(hexSchema)
            })
            .optional()
    })
    .strict();

const callResponseSchema = z.object({
    jsonrpc: z.literal('2.0'),
    id: z.any(),
    result: hexSchema.optional()
});

export function validatedEthereumCall(name: string) {
    return {
        name: name,
        async handle(
            context: RequestContext,
            method: string,
            params: unknown[],
            id: number | string
        ): Promise<FastifyReplyType> {
            if (env.PERMISSIONS_HOT_RELOAD === 'true') {
                authorizer.reloadFromEnv();
            }
            const call = callReqSchema.parse(params[0]);
            if (params.length > 2) {
                return unauthorized(id, 'state overrides are not supported');
            }

            if (call.from !== undefined && !isAddressEqual(call.from, context.currentUser)) {
                return unauthorized(id);
            }

            const data = call.data || call.input;
            if (data && !context.authorizer.checkContractRead(call.to, data, context.currentUser)) {
                return unauthorized(id);
            }

            const rule = data && context.authorizer.checkPostReadFilter(call.to, data);
            if (rule) {
                const res = await sendToTargetRpc(context.targetRpcUrl, id, method, params, callResponseSchema);
                if (!res.result) {
                    return unauthorized(id);
                }

                if (rule.canRead(context.currentUser, res.result)) {
                    return res;
                } else {
                    return unauthorized(id);
                }
            }

            return delegateCall({ url: context.targetRpcUrl, id, method, params });
        }
    };
}
