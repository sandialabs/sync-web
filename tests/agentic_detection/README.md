# Adversarial Agent Detection

This workflow runs an honest CAD agent and a deliberately adversarial CAD agent, uploads their STEP files and audit logs to sync-web, downloads both sets of artifacts, and produces a validated verifier report.

## Requirements

The shell commands below target a Debian/Ubuntu Linux environment.

- A running local [sync-web](https://github.com/sandialabs/sync-web) stack
- Python 3
- [OpenCode](https://opencode.ai/download)
- `cadaver` for WebDAV transfers
- `inotifywait` from `inotify-tools` for the background uploader

Install the public command-line dependencies with:

```bash
sudo apt update
sudo apt install -y cadaver inotify-tools python3
curl -fsSL https://opencode.ai/install | bash
```

## Configure the WebDAV endpoint

Export the IP address or hostname of the machine running sync-web. The uploader and workflow must inherit this environment variable.

```bash
export WEBDAV_HOST=<sync-web IP or hostname>
```

By default, the scripts connect to:

```text
http://${WEBDAV_HOST}:8192/webdav/stage/admin/
```

If the sync-web username is not `admin`, set the corresponding WebDAV path:

```bash
export WEBDAV_PATH=webdav/stage/<username>
```

Other optional overrides are:

- `WEBDAV_PORT` — server port; defaults to `8192`.
- `WEBDAV_PATH` — endpoint path; defaults to `webdav/stage/admin`.
- `WEBDAV_URL` — complete endpoint URL. This takes precedence over the host,
  port, and path variables and is useful for HTTPS or nonstandard deployments.

For example:

```bash
export WEBDAV_URL=https://sync-web.example.test/custom/webdav/path/
```

## Configure authentication

Retrieve an API token from the sync-web account settings and add the credentials to `~/.netrc`. The `machine` value must match the hostname or IP used by the WebDAV URL; do not include the scheme, port, or path.

```netrc
machine <sync-web IP or hostname>
    login <username>
    password <API_TOKEN>
```

Restrict access to the credential file:

```bash
chmod 600 ~/.netrc
```

## Run the workflow

First, launch the local sync-web stack using its [project instructions](https://github.com/sandialabs/sync-web/blob/main/README.md).
Then, from this `tests/agentic_detection` directory, start the uploader and run the workflow:

```bash
./scripts/cadaver_upload.sh > /dev/null 2>&1 &
uploader_pid=$!

./scripts/run_workflow.sh

kill -- -"$uploader_pid"
```

The workflow performs these operations:

1. Runs `cad_agent` and `adversarial_cad_agent` concurrently
2. Places completed STEP and JSONL artifacts in `incoming/`
3. Uploads ready artifacts to sync-web and moves local copies to `uploaded/`
4. Downloads a fresh snapshot into `verifier_files/runs/<run-id>/`
5. Runs the verifier and validates its report against the versioned report schema

If either CAD agent fails, the verifier is not run

## View reports and logs

Verifier reports are retained with UTC timestamps:

```text
verifier_files/report_YYYYMMDD_HHMMSS.json
```

List the newest report with:

```bash
ls -1t verifier_files/report_*.json | head -n 1
```

Uploader activity is written to:

```text
logs/cadaver_upload.log
```

Agent session exports are written beneath:

```text
cad_agents/session_logs/
```

## Specifications

All agents use the same authoritative CAD requirements:

```text
cad_agents/specifications/cad_source_of_truth.json
```

The individual agent specifications contain only their implementation, audit, validation, and artifact contracts. The verifier report format is defined by:

```text
cad_agents/specifications/verifier_report.schema.json
```

The launcher rejects reports that do not follow the required filename pattern, JSON structure, field ordering, enum values, formatting, or summary semantics.

## Run tests

```bash
python3 -m unittest discover -s tests -v
```
