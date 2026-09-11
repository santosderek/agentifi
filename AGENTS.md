# Agentifi Development Guide

## Project purpose

Agentifi is a Rust workspace for controlling coding-agent sessions through two cross-platform applications:

- `agentifi-server`: runs beside agent sessions and exposes the control API.
- `agentifi-desktop`: connects to a local or remote Agentifi server.

The server is the owner of machine-local session discovery and process control. The desktop client must not read server-side session files directly.

## Repository layout

```text
apps/agentifi-server       Axum server binary
apps/agentifi-desktop      egui desktop binary
crates/domain              Domain entities and invariants
crates/application         Application use cases
crates/ports               Hexagonal port traits
crates/adapters            Infrastructure implementations
docs/                      Product, architecture, user, and contribution docs
Justfile                   Local build recipes
```

## Required architecture

Use domain-driven design and hexagonal architecture:

- Keep business concepts and invariants in `crates/domain`.
- Keep use-case orchestration in `crates/application`.
- Define external boundaries as traits in `crates/ports`.
- Put filesystem, process, database, HTTP, WebSocket, and platform code in `crates/adapters` or application-specific adapters.
- Keep Axum handlers thin and transport-specific.
- Keep egui state and widgets out of the domain model.
- Do not make domain code depend on Axum, egui, Tokio, filesystem paths, or Pi-specific record formats.
- Add a port before adding a second implementation of an external concern.

Read `docs/ARCHITECTURE.md` and `docs/PLAN.md` before changing boundaries.

## Toolchain and build commands

This SteamOS environment has a Homebrew GCC shim that does not have access to the system C headers needed by crates such as `ring`. Use the repository recipes, which select the mise-managed Clang toolchain:

```bash
just cli-build       # Build agentifi-server
just desktop-build  # Build agentifi-desktop
just desktop         # Build, then run the desktop client
```

For validation:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

If running Cargo directly on this machine, use:

```bash
CC=clang \
CXX=clang++ \
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang \
cargo check --workspace
```

Do not “fix” the `ring` compiler error by adding generated files or committing local toolchain paths.

## Local runtime behavior

The server defaults to:

```text
127.0.0.1:8787
```

Override the bind address with `AGENTIFI_BIND`.

When the desktop starts, it checks the local server first. If one is available, it connects without spawning another server. If no local server responds, it starts `agentifi-server`, waits for readiness, and owns that child process until the desktop exits.

Set `AGENTIFI_SERVER_COMMAND` when the server executable is installed outside the normal development layout.

The current API slice includes:

```text
GET /health
GET /api/v1/sessions
```

The in-memory adapter and seeded session are scaffolding. Replace them with real Pi session discovery through a port; do not put filesystem parsing in the HTTP handler.

## Testing expectations

For each feature:

1. Add domain tests for business rules.
2. Add application tests using fake ports.
3. Add adapter/integration tests for filesystem, process, persistence, or transport behavior.
4. Add API compatibility tests for endpoint changes.
5. Test failure, timeout, cancellation, and reconnect paths.
6. Run `cargo fmt --all -- --check`, `cargo check --workspace`, and `cargo test --workspace`.

Do not commit personal Pi sessions, generated databases, credentials, tokens, private keys, cookies, or machine-specific `.env` files.

## Security rules

- Loopback-only is the default and safest server mode.
- Remote binding must require explicit configuration and authenticated pairing.
- Never add arbitrary remote shell execution to the API.
- Store desktop credentials in the OS credential store when implemented.
- Keep server-side provider credentials in the server's secret store.
- Redact secrets, cookies, private keys, and session content from logs.
- Mutating operations must have capability checks, confirmation semantics where appropriate, and audit events.

## Change workflow

Before editing:

```bash
git status --short --branch
```

After editing:

```bash
cargo fmt --all
git diff --check
just cli-build
just desktop-build
cargo test --workspace
git status --short --branch
```

Keep commits focused and use descriptive messages. Update the relevant document when changing architecture, API behavior, security assumptions, or user-facing workflows. Push only intentional commits to `origin/main` after validation.
