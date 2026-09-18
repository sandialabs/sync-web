FROM nginx:1.27-alpine

RUN apk add --no-cache gettext
COPY deploy/nginx.conf.template /opt/messenger/nginx.conf.template
COPY deploy/entrypoint.sh /usr/local/bin/messenger-entrypoint
COPY public /usr/share/nginx/html
RUN chmod 0755 /usr/local/bin/messenger-entrypoint \
    && find /usr/share/nginx/html -type d -exec chmod 0755 {} + \
    && find /usr/share/nginx/html -type f -exec chmod 0644 {} +

ENTRYPOINT ["/usr/local/bin/messenger-entrypoint"]
