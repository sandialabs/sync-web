ARG JOURNAL_SDK_IMAGE=ghcr.io/sandialabs/sync-web/journal-sdk:latest
FROM ${JOURNAL_SDK_IMAGE}

ARG CUSTOM_SETUP=""

RUN set -eu; \
    if [ -n "$CUSTOM_SETUP" ]; then \
      echo "$CUSTOM_SETUP" | base64 -d > /tmp/custom-setup.sh; \
      chmod +x /tmp/custom-setup.sh; \
      /bin/sh -eu /tmp/custom-setup.sh; \
      rm -f /tmp/custom-setup.sh; \
    fi

COPY records/lisp/*.scm /srv/
COPY deploy/compose/general/run.sh ./

ENTRYPOINT ["./run.sh"]
