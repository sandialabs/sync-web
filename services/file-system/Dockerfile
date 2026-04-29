FROM mcr.microsoft.com/dotnet/sdk:10.0 AS build

WORKDIR /src

ARG CUSTOM_SETUP=""

RUN set -eu; \
    if [ -n "$CUSTOM_SETUP" ]; then \
      echo "$CUSTOM_SETUP" | base64 -d > /tmp/custom-setup.sh; \
      chmod +x /tmp/custom-setup.sh; \
      /bin/sh -eu /tmp/custom-setup.sh; \
      rm -f /tmp/custom-setup.sh; \
    fi

COPY Directory.Build.props ./
COPY FileSystem.sln ./
COPY src/FileSystem.Server/FileSystem.Server.csproj src/FileSystem.Server/
COPY tests/FileSystem.Server.Tests/FileSystem.Server.Tests.csproj tests/FileSystem.Server.Tests/

RUN dotnet restore FileSystem.sln

COPY src ./src
COPY tests ./tests

RUN dotnet build FileSystem.sln -c Release --no-restore
RUN dotnet publish src/FileSystem.Server/FileSystem.Server.csproj -c Release --no-build -o /app/publish

FROM build AS test

RUN dotnet test FileSystem.sln -c Release --no-build

FROM mcr.microsoft.com/dotnet/runtime:10.0 AS runtime

WORKDIR /app

ARG CUSTOM_SETUP=""

RUN set -eu; \
    if [ -n "$CUSTOM_SETUP" ]; then \
      echo "$CUSTOM_SETUP" | base64 -d > /tmp/custom-setup.sh; \
      chmod +x /tmp/custom-setup.sh; \
      /bin/sh -eu /tmp/custom-setup.sh; \
      rm -f /tmp/custom-setup.sh; \
    fi

ENV SYNC_FS_Mode=bootstrap
ENV SYNC_FS_Port=445
ENV SYNC_FS_ShareName=sync
ENV SYNC_FS_StaticRoot=/srv/share
ENV SYNC_FS_Backend=json
ENV SYNC_FS_JsonFixturePath=/workspace/tests/static-tree.json
ENV SYNC_FS_EnableSmb1=false
ENV SYNC_FS_EnableSmb2=true
ENV SYNC_FS_EnableSmb3=false
ENV SYNC_FS_AllowGuest=true
ENV SYNC_FS_GuestAccountName=Guest
ENV SYNC_FS_GatewayBaseUrl=http://gateway/api/v1
ENV SYNC_FS_GatewayTimeoutMs=30000

COPY --from=build /app/publish ./

ENTRYPOINT ["dotnet", "FileSystem.Server.dll"]
