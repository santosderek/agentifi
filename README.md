# Agentifi

Agentifi is a cross-platform control plane for Pi coding-agent sessions. A small server runs beside the sessions and exposes a secure, versioned API. A desktop client connects to one or more servers to browse and control those sessions.

The repository is intentionally a Rust workspace with domain-driven boundaries from the first commit. The initial implementation is a thin vertical slice: the server exposes health and session-list endpoints, and the desktop client can connect to and display them. The architecture is designed to grow without coupling the UI or transport to Pi-specific details.

## Workspace

- `apps/agentifi-server` — machine-local or remote Agentifi server.
- `apps/agentifi-desktop` — cross-platform desktop client.
- `crates/domain` — business entities and invariants.
- `crates/application` — use cases and orchestration.
- `crates/ports` — inbound/outbound interfaces.
- `crates/adapters` — replaceable infrastructure implementations.
- `docs/` — product, architecture, security, and development plans.

## Quick start

```bash
cargo run -p agentifi-server
cargo run -p agentifi-desktop
```

The server listens on `127.0.0.1:8787` by default. Override it with `AGENTIFI_BIND`. The desktop client defaults to `http://127.0.0.1:8787`: it connects to an existing local server when available, otherwise starts the local `agentifi-server` executable and connects to it. Set `AGENTIFI_SERVER_COMMAND` when the server binary is installed in a nonstandard location.

## Project status

This is an architectural foundation, not a finished remote-control product. Authentication, authorization, TLS, session discovery, Pi integration, durable storage, and process supervision are specified in the plan and deliberately not faked in the initial slice.

Read [the plan](docs/PLAN.md) and [the architecture](docs/ARCHITECTURE.md) before changing boundaries.

## License

MIT. See [LICENSE](LICENSE).
