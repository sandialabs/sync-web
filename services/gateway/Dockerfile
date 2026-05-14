FROM node:20-alpine AS ui-build

WORKDIR /app/ui

COPY ui/package*.json ./
RUN npm ci

COPY ui/ ./
RUN npm run build

FROM ui-build AS ui-test
RUN npm test

FROM node:20-alpine AS build

WORKDIR /app

ARG CUSTOM_SETUP=""

RUN set -eu; \
    if [ -n "$CUSTOM_SETUP" ]; then \
      echo "$CUSTOM_SETUP" | base64 -d > /tmp/custom-setup.sh; \
      chmod +x /tmp/custom-setup.sh; \
      /bin/sh -eu /tmp/custom-setup.sh; \
      rm -f /tmp/custom-setup.sh; \
    fi

COPY package*.json ./
RUN npm ci

COPY tsconfig.json ./
COPY assets ./assets
COPY src ./src
COPY test ./test
RUN npm run build:server

FROM build AS test
COPY --from=ui-test /app/ui/dist /tmp/auth-ui-dist
RUN npm run test:server

FROM node:20-alpine AS runtime

WORKDIR /app
ENV NODE_ENV=production

ARG CUSTOM_SETUP=""

RUN set -eu; \
    if [ -n "$CUSTOM_SETUP" ]; then \
      echo "$CUSTOM_SETUP" | base64 -d > /tmp/custom-setup.sh; \
      chmod +x /tmp/custom-setup.sh; \
      /bin/sh -eu /tmp/custom-setup.sh; \
      rm -f /tmp/custom-setup.sh; \
    fi

COPY package*.json ./
RUN npm ci --omit=dev

COPY assets ./assets
COPY --from=build /app/dist ./dist
COPY --from=ui-build /app/ui/dist ./ui/dist

EXPOSE 8180

CMD ["node", "dist/server.js"]
