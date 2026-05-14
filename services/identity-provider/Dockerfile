FROM oryd/kratos:v1.3.1

USER root

COPY identity.schema.json /etc/config/kratos/identity.schema.json
COPY docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod a+w /etc/config/kratos

ENTRYPOINT ["/docker-entrypoint.sh"]
CMD ["kratos", "serve", "-c", "/etc/config/kratos/kratos.yml"]
