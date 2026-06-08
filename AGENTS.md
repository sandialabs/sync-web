# Agent Guidance

## Git

- Do all work on separate branches, never push or otherwise edit `main` directly.
- Do not push any branch unless explicitly asked.
- All pull requests to `main` should consist of a single squashed commit with a detailed markdown message.

## Versioning

- The platform version in `/VERSION` and the journal-sdk crate version is in `journal/Cargo.toml`.
  - Platform version should be updated for all pull requests
  - Crate version should only be updated for pull requests that affect the journal-sdk
- For both notions of version:
  - Bump the relevant version for every pull request. Confirm the type first: patch, minor, or (rarely) major.
  - When bumping version, update all deployment configurations to reference the latest version.
  - When bumping minor or major versions, update the changelog if one already exists.

## Documentation
- Before final pull requests, ensure that documentation in `/docs` track the implementation
- Before final pull requests, ensure that README.md and other repo-level documents track the implementation.
- When adding or changing modules, classes, or functions, ensure that docstrings are correct and consistent.

## Deployment

- Preserve both deployment paths:
  - `deploy/compose/general/compose.yaml` is the primary single-node deployment path.
  - `tests/api/local-compose.sh` is the primary local smoke-test path.
- Preserve both TLS and plaintext deployment paths
  - HTTP-only deployment must keep working for internal networks.
  - TLS deployment must keep working with host-provided certificate/key paths.
- Preserve both Docker, Podman, and other standard container runtimes where possible.
- Consider Windows, macOS, and Linux (Ubuntu and Fedora SELinux) compatibility posture where possible.

## Testing

- Before submitting pull requests, identify and run Github Actions locally where relevant and possible.
  - Relevant actions are in .github/workflows/*.yml.

## Development

- When working with s7 scheme/lisp code, consult `/records/LANGUAGE.md`
- In Scheme, avoid polluting local namespaces with low-value intermediate variables when collection references are short and explicit.
- Prefer exact expected data shapes that fail quickly when malformed over networks of ad hoc fallbacks that become hard to reason about.
- For high-blast-radius changes, roll out from lower layers to higher layers in separate commits/PR slices where practical: Rust/journal changes first, then Scheme record changes, then service/UI changes. Run/update relevant tests incrementally with each slice.
