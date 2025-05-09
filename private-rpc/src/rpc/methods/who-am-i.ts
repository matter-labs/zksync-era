import { MethodHandler } from '@/rpc/rpc-service';
import { response } from '@/rpc/json-rpc';

export const whoAmI: MethodHandler = {
    name: 'who_am_i',
    async handle(context, _method, _params, id) {
        return response({ id, result: context.currentUser });
    }
};
