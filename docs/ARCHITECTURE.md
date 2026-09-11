# Architecture

## Boundaries

Agentifi follows domain-driven design and hexagonal architecture.

```text
Desktop UI / HTTP / WebSocket
          │
      inbound adapters
          │
     application use cases
          │
       domain model
          │
      outbound ports
          │
Pi filesystem / process supervisor / database / event bus
```

### Domain

The domain owns concepts such as `AgentSession`, `Project`, `Machine`, `SessionEvent`, `Capability`, and `Action`. It must not import Axum, egui, Tokio, SQLite, filesystem paths, or Pi implementation details.

The domain model should represent stable business meaning, not mirror an on-disk JSON document. Provider-specific data belongs behind provider adapters or an explicitly versioned metadata envelope.

### Application

Application services implement use cases: list sessions, inspect a session, pair a client, resume a session, stop a session, and subscribe to events. They coordinate ports, enforce authorization-relevant policies, and return domain results. They do not know whether a request came from HTTP or the desktop UI.

### Ports

Ports are traits owned by the core:

- `SessionRepository` — discover and query sessions.
- `SessionController` — perform lifecycle actions.
- `AgentProvider` — discover capabilities and translate provider operations.
- `ProcessSupervisor` — start, monitor, cancel, and reap controlled processes.
- `EventPublisher` — publish durable and transient events.
- `PairingStore` — issue, rotate, revoke, and validate device credentials.
- `Clock` and `IdGenerator` — deterministic tests.

Ports should express business operations, not transport-shaped DTOs.

### Adapters

Adapters implement ports:

- Pi session filesystem adapter.
- Process supervisor adapter.
- SQLite or another durable store.
- Axum HTTP/WebSocket adapter.
- Desktop HTTP/WebSocket client adapter.
- OS keychain adapter for desktop credentials.

Adapters may fail, time out, or be unavailable. Failures must be mapped into stable application errors before crossing the API boundary.

## Server topology

The server runs next to the agent sessions. It owns discovery, authorization, process lifecycle, event fan-out, and audit records. The desktop client never reads the server's session files directly.

Default mode binds to loopback. A remote bind requires explicit configuration and secure pairing. A future deployment may place the server behind a private network or an authenticated tunnel, but the protocol must not depend on a specific tunnel vendor.

## Protocol principles

- `/api/v1` is explicit and backward-compatible.
- Commands are idempotent where possible and carry request IDs.
- Long-running actions return an operation ID and stream progress.
- Every mutating operation produces an audit event.
- API responses contain capability and version information.
- The server never returns secret values from its environment or provider credentials.

## Data and consistency

Session discovery is eventually consistent. The API should expose `observed_at`, source revision, and stale/error state. Mutations are authoritative only after the process supervisor confirms them. Clients must render pending, succeeded, failed, and unknown outcomes separately.

## Security model

- Loopback-only is the safe default.
- Pairing grants a scoped device credential, not a reusable account password.
- Credentials are stored in the OS keychain where available.
- Server-side secrets remain in the server's secret store.
- Actions require explicit capability checks and are audited.
- Logs must redact tokens, cookies, private keys, and session content unless the user explicitly requests a safe diagnostic.
- Updates and binaries should be checksum/signature verified in release automation.

## Testing strategy

- Domain: pure unit tests and property tests.
- Application: fake ports and deterministic clocks.
- Adapters: contract tests plus platform-specific integration tests.
- API: request/response compatibility tests.
- Desktop: view-model tests plus a small number of end-to-end smoke tests.
- Security: authorization matrix tests and secret-redaction tests.
