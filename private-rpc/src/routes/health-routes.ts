import { WebServer } from '@/build-app';

export function healthRoutes(app: WebServer) {
    app.get('/', async (_req, reply) => {
        return reply.send({ ok: true });
    });
}
