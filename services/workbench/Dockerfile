FROM node:20-alpine3.20 AS deps

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
RUN npm install

FROM deps AS test

COPY . .
RUN npm test -- --watchAll=false

FROM deps AS build

COPY . .
RUN npm run build

FROM nginx:1.30.1-alpine3.23@sha256:c819f83c54b0361f5557601bf5eb4943d09360e7a7fdf426afc466570f45874d AS runtime

ARG CUSTOM_SETUP=""

RUN set -eu; \
    if [ -n "$CUSTOM_SETUP" ]; then \
      echo "$CUSTOM_SETUP" | base64 -d > /tmp/custom-setup.sh; \
      chmod +x /tmp/custom-setup.sh; \
      /bin/sh -eu /tmp/custom-setup.sh; \
      rm -f /tmp/custom-setup.sh; \
    fi; \
    rm -rf /usr/share/nginx/html/*

COPY --from=build /app/build /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY docker-entrypoint.sh /sync-entrypoint.sh
RUN chmod +x /sync-entrypoint.sh

EXPOSE 80

ENTRYPOINT ["/sync-entrypoint.sh"]
CMD ["nginx", "-g", "daemon off;"]
