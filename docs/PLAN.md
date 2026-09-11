# Agentifi Product and Delivery Plan

## Vision

Agentifi gives a person a trustworthy way to discover, inspect, resume, pause, and eventually control Pi sessions running on another machine. The server belongs with the sessions and operating-system resources; the desktop client is a replaceable operator console.

The first release should favor a small, inspectable protocol and excellent local development over feature breadth. Remote access must be opt-in, authenticated, encrypted, and observable.

## User stories

1. As an operator, I can install the server on a machine where Pi runs.
2. As an operator, I can pair a desktop client with a server without copying private credentials into a project file.
3. As an operator, I can see projects, sessions, status, timestamps, branches, and recent activity.
4. As an operator, I can resume a session in the correct project context.
5. As an operator, I can request pause, stop, archive, export, or terminate operations with clear confirmation.
6. As an operator, I can manage multiple trusted machines and see which machine owns each session.
7. As a developer, I can add a new agent backend without rewriting the desktop UI or transport layer.

## Milestones

### M0 — Foundation (current)

- Rust workspace with server and desktop binaries.
- Domain/application/ports/adapters boundaries.
- Health and session-list vertical slice.
- Public API and architecture documentation.

### M1 — Local Pi session discovery

- Define a `SessionSource` port and Pi filesystem adapter.
- Parse Pi session records defensively, including branches and compaction metadata.
- Add stable identifiers, project identity, timestamps, and read-only search.
- Add fixtures and golden tests for real session shapes without committing personal data.

### M2 — Server API and live updates

- Versioned REST endpoints for pairing, machines, projects, sessions, messages, and actions.
- WebSocket or server-sent event stream for session changes and process output.
- Cursor-based pagination and idempotent commands.
- OpenAPI document generated from the transport adapter.

### M3 — Secure pairing

- Local-only mode by default.
- Explicit remote bind configuration.
- One-time pairing code or QR flow, device identity, revocation, and scoped capabilities.
- TLS or a documented secure tunnel requirement; never silently expose a plaintext control API.

### M4 — Session control

- Resume through a process-supervisor port rather than shelling out from HTTP handlers.
- Pause/stop/terminate with policy checks and confirmation.
- Structured command results, audit events, timeouts, cancellation, and recovery after server restart.

### M5 — Desktop workbench

- Server list and pairing UX.
- Session browser, project grouping, tree/history view, live activity, and action confirmations.
- Offline cache with explicit stale-data indicators.
- Cross-platform packaging for Linux, macOS, and Windows.

### M6 — Extensibility

- Agent provider port and first-class Pi provider.
- Provider capability negotiation.
- Plugin boundary only after the core domain is stable; plugins must not bypass authorization or audit logging.
- Optional integrations for other coding agents without contaminating the Pi domain model.

## Non-goals for the first release

- Arbitrary remote shell access.
- Automatic public internet exposure.
- Storing provider API keys in the server database.
- Pretending every agent has the same session format.
- Building a general-purpose terminal multiplexer into the desktop app.

## Definition of done for a feature

- Domain behavior has unit tests.
- Port contracts have tests with fakes.
- Adapters have integration tests and failure-path coverage.
- API changes are documented and versioned.
- Security impact and required permissions are documented.
- A user can understand how to recover from failure.
