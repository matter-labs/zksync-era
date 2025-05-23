import { JsonRpcRequest, request } from './json-rpc';

export async function delegateCall({
    url,
    id,
    method,
    params
}: {
    url: string;
    id: JsonRpcRequest['id'];
    method: JsonRpcRequest['method'];
    params: JsonRpcRequest['params'];
}) {
    const response = await fetch(url, {
        method: 'POST',
        body: JSON.stringify(request({ id, method, params })),
        headers: { 'Content-Type': 'application/json' }
    });
    // you can't just use response.body, which would be much faster
    // because the framework doesn't handle it nicely in case of batch requests
    // TODO research if that can be improved
    return response.json();
}
