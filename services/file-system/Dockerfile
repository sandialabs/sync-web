# syntax=docker/dockerfile:1

FROM docker.io/library/golang:1.23-alpine AS base
ARG CUSTOM_SETUP=""
WORKDIR /src
RUN set -eu; \
    if [ -n "$CUSTOM_SETUP" ]; then \
      echo "$CUSTOM_SETUP" | base64 -d > /tmp/custom-setup.sh; \
      chmod +x /tmp/custom-setup.sh; \
      /bin/sh -eu /tmp/custom-setup.sh; \
      rm -f /tmp/custom-setup.sh; \
    fi
COPY go.mod ./
RUN go mod download
COPY . .

FROM base AS test
RUN go test ./...

FROM base AS build
RUN CGO_ENABLED=0 GOOS=linux go build -trimpath -ldflags='-s -w' -o /out/file-system ./cmd/file-system

FROM gcr.io/distroless/static-debian12 AS runtime
COPY --from=build /out/file-system /file-system
EXPOSE 8080
ENTRYPOINT ["/file-system"]
