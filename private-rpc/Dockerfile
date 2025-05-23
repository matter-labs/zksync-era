FROM node:22-slim
WORKDIR /app

COPY src src/
COPY package.json .
COPY drizzle.config.ts .
COPY esbuild.ts .
COPY tsconfig.json .

RUN yarn install
RUN yarn build

CMD [ "node", "dist/index.js" ]
