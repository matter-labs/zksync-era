import { Hex } from 'viem';
import { MethodHandler, RequestContext } from '@/rpc/rpc-service';
import { FastifyReplyType } from 'fastify/types/type-provider';
import { request, unauthorized } from '@/rpc/json-rpc';
import { delegateCall } from '@/rpc/delegate-call';
import { z, ZodTypeAny } from 'zod';
import { addressSchema } from '@/schemas/address';

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
