# Security Policy

Please do not open public issues for suspected vulnerabilities.

Use GitHub private vulnerability reporting if it is enabled for this repository. If it is not available, contact the project maintainers through an appropriate private channel and include enough detail to reproduce or assess the issue.

## Scope

Security-relevant reports include, but are not limited to:

- authentication, session, or API-token bypasses
- journal/interface authorization bugs
- bridge trust, proof, signature, or synchronization integrity issues
- secret exposure in configuration, logs, release artifacts, documentation, or public APIs
- deployment defaults that expose privileged operations unintentionally
- vulnerabilities in release, container, or binary distribution workflows

Dependency scanner findings are most useful when accompanied by exploitability or reachability context for this project.

## Supported versions

Unless otherwise stated, security fixes target the current `main` branch and latest published release.

## Handling sensitive material

Do not include real secrets, private keys, access tokens, private journal data, database dumps, or production logs containing sensitive content in public reports. Redact secrets from command output and logs. Coordinate privately before sharing sensitive artifacts.
