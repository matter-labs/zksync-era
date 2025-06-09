FROM node:22-slim
WORKDIR /app

COPY package.json .
RUN yarn install

COPY src src/
COPY drizzle.config.ts .
COPY drizzle drizzle
COPY esbuild.ts .
COPY tsconfig.json .

RUN yarn build

CMD [ "node", "dist/index.js" ]
