FROM node:22-slim AS build
RUN apt-get update && apt-get install -y python3 make g++
WORKDIR /app

COPY src src/
COPY package.json .
COPY drizzle.config.ts .
COPY esbuild.ts .
COPY .eslintrc.cjs .
COPY tsconfig.json .

RUN yarn install
RUN yarn build

COPY example-permissions.yaml .

CMD [ "node", "dist/index.js" ]
