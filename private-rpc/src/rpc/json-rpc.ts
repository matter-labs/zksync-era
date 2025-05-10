export type JsonRpcRequest = {
    jsonrpc: '2.0';
    id: number | string;
    method: string;
    params: unknown[];
};

export type JsonRpcResponse = {
    jsonrpc: '2.0';
} & (
    | {
          id: number | string;
          result: JSONLike;
      }
    | {
          id: number | string | null;
          error: JsonRpcError;
      }
);

export type JsonRpcError = {
    code: number;
    message: string;
    data?: unknown;
};

type JSONLike =
    | {
          [key: string]: JSONLike;
      }
    | string
    | number
    | null
    | boolean
    | JSONLike[];

export function unauthorized(id: JsonRpcResponse['id'], message = 'Unauthorized'): JsonRpcResponse {
    return {
        jsonrpc: '2.0',
        id,
        error: {
            code: -32090,
            message
        }
    };
}

export function invalidRequest(id: JsonRpcResponse['id']): JsonRpcResponse {
    return {
        jsonrpc: '2.0',
        id,
        error: { code: -32600, message: 'Invalid Request' }
    };
}

export function request({
    id,
    method,
    params
}: {
    id: JsonRpcRequest['id'];
    method: JsonRpcRequest['method'];
    params: JsonRpcRequest['params'];
}): JsonRpcRequest {
    return {
        jsonrpc: '2.0',
        id,
        method,
        params
    };
}

export function response({
    id,
    result
}: {
    id: Exclude<JsonRpcResponse['id'], null>;
    result: JSONLike;
}): JsonRpcResponse {
    return {
        jsonrpc: '2.0',
        id,
        result
    };
}

export function errorResponse({ id, error }: { id: JsonRpcResponse['id']; error: JsonRpcError }): JsonRpcResponse {
    return {
        jsonrpc: '2.0',
        id,
        error
    };
}
