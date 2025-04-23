import { FastifyError } from '@fastify/error';

export class HttpError extends Error implements FastifyError {
    statusCode: number;
    code: string;

    constructor(msg: string, status: number) {
        super(msg);
        this.code = msg.toLowerCase();
        this.statusCode = status;
    }
}
