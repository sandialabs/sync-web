FROM python:3.11.9-alpine3.19

WORKDIR /srv
ARG CUSTOM_SETUP=""

RUN set -eu; \
    if [ -n "$CUSTOM_SETUP" ]; then \
      echo "$CUSTOM_SETUP" | base64 -d > /tmp/custom-setup.sh; \
      chmod +x /tmp/custom-setup.sh; \
      /bin/sh -eu /tmp/custom-setup.sh; \
      rm -f /tmp/custom-setup.sh; \
    fi

COPY docker/social-agent/requirements.txt ./requirements.txt

RUN pip install -r requirements.txt

COPY docker/social-agent/frankenstein.txt ./frankenstein.txt
COPY docker/social-agent/run.py ./run.py

CMD ["python", "run.py"]
