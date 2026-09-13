# Pi Control Protocol

This document defines how the desktop, Agentifi server, and Pi RPC process cooperate.

## Roles

```text
┌────────────────┐       HTTP JSON-RPC       ┌──────────────────┐
│ Agentifi       │◀─────────────────────────▶│ Agentifi server  │
│ desktop        │                           │ local daemon     │
└───────┬────────┘       SSE events          └────────┬─────────┘
        │                                             │
        │                                             │ Pi RPC JSONL
        │                                             ▼
        │                                      ┌──────────────┐
        └──────────── session view ───────────▶│ pi --mode   │
                                               │ rpc process  │
                                               └──────────────┘
```

The desktop never launches Pi directly. The server owns Pi process lifetime, session-file access, command serialization, and event fan-out.

The server is the local CLI/daemon layer. A future standalone `agentifi` CLI can call the same server API for diagnostics and scripting.

## Session discovery flow

```text
1. PiSessionScanner locates the configured Pi session directory.
2. Scanner reads session metadata and JSONL headers.
3. Session records are normalized into Agentifi domain entities.
4. SessionRepository stores the current catalog.
5. SessionEventPublisher emits discovered/updated/removed events.
6. Desktop receives events through the SSE endpoint.
7. Desktop updates the machine/session list without polling.
```

Discovery does not launch a Pi process. A Pi RPC process is created only when the user attaches to or controls a session.

## Desktop connection state

```text
Starting
   │
   ▼
CheckingLocal ──healthy──▶ Connected
   │                         │
   │ unavailable              │ SSE disconnect
   ▼                         ▼
StartingLocalServer       Reconnecting
   │                         │
   │ ready                   │ retry exhausted
   ▼                         ▼
Connected ◀────────────── Connected / Offline
```

The desktop owns a locally spawned server child only when it started that child. An already-running server is never stopped by the desktop.

## Server session state

```text
Discovered
    │ attach
    ▼
StartingPiRpc
    │ process ready
    ▼
Attached ───── prompt/steer ─────▶ Running
    │                                  │
    │                                  │ response + events
    │                                  ▼
    └────────────── abort ────────▶ Idle
                                       │
                                       │ detach / process exit
                                       ▼
                                    Detached
```

Failure transitions from every active state to `Failed`, carrying a stable error code and a human-readable message. Recovery is explicit and can restart the Pi RPC process.

## Desktop-to-server JSON-RPC

Endpoint:

```text
POST /api/v1/rpc
Content-Type: application/json
```

Request envelope:

```json
{
  "jsonrpc": "2.0",
  "id": "req-123",
  "method": "sessions.attach",
  "params": {
    "session_id": "session-id"
  }
}
```

Success envelope:

```json
{
  "jsonrpc": "2.0",
  "id": "req-123",
  "result": {
    "session_id": "session-id",
    "state": "attached"
  }
}
```

Error envelope:

```json
{
  "jsonrpc": "2.0",
  "id": "req-123",
  "error": {
    "code": "SESSION_NOT_FOUND",
    "message": "The session is no longer present",
    "data": {}
  }
}
```

### Initial methods

| Method | Purpose | Status |
|---|---|---|
| `sessions.list` | Query the discovered session catalog with live attachment/working state | Implemented |
| `sessions.get` | Read session metadata and current state | Implemented |
| `sessions.attach` | Start or reuse a Pi RPC process for a session | Implemented |
| `sessions.detach` | Stop the server-side attachment | Implemented |
| `sessions.state` | Request current Pi state (`get_state`) | Implemented |
| `sessions.messages` | Live conversation from the attached process, or the stored transcript | Implemented |
| `sessions.entries` | Session entries with a durable cursor (`get_entries`) | Implemented |
| `sessions.prompt` | Send a prompt to Pi | Implemented |
| `sessions.steer` | Queue an interrupting steering message | Implemented |
| `sessions.follow_up` | Queue a follow-up message | Implemented |
| `sessions.abort` | Abort the active Pi operation | Implemented |
| `sessions.clear_queue` | Clear queued steering/follow-up messages | Implemented |
| `sessions.set_model` | Change the active model | Implemented |
| `machines.list` | List known local and paired machines | Planned |
| `sessions.new` | Create a new Pi session | Planned |
| `operations.get` | Inspect a long-running operation | Planned |
| `operations.cancel` | Cancel a server-side operation | Planned |

## Server-to-Pi RPC mapping

The server starts an attached session with an explicit session path:

```text
pi --mode rpc --session <session-file>
```

Pi RPC uses strict JSONL over stdin/stdout (`https://pi.dev/docs/latest/rpc`). Agentifi maintains one supervised process connection per attached session. Attach proves readiness by round-tripping a `get_state` command before reporting success.

Commands the server issues (correlated by request id):

```json
{"id":"agentifi-…","type":"get_state"}
{"id":"agentifi-…","type":"get_messages"}
{"id":"agentifi-…","type":"get_entries","since":"<last entry id>"}
{"id":"agentifi-…","type":"prompt","message":"Inspect the failing test"}
{"id":"agentifi-…","type":"steer","message":"Stop and summarize the current state"}
{"id":"agentifi-…","type":"follow_up","message":"After that, run the tests"}
{"id":"agentifi-…","type":"abort"}
{"id":"agentifi-…","type":"clear_queue"}
{"id":"agentifi-…","type":"set_model","provider":"anthropic","modelId":"claude-sonnet-4"}
```

Responses arrive as `{"type":"response","command":…,"success":…,"data":…}` frames carrying the same `id`. Failures after acceptance are reported through the agent event stream, not as a second response.

The adapter must:

- Write only complete LF-delimited JSON records.
- Correlate response records by request ID.
- Forward asynchronous Pi events to the Agentifi event publisher.
- Preserve Pi event ordering per session.
- Apply timeouts (10s per command) and cancellation without killing unrelated sessions.
- Redact credentials and provider data from logs.

### Working-state tracking

The supervisor derives live status from Pi's agent events:

- `agent_start`, `message_start`, `tool_execution_start` mark the session `running`.
- `agent_settled` is the authoritative "fully done" signal and marks it `idle`.
- Each transition emits `session.updated` so clients refresh without polling.
- Streaming deltas (`message_update`) are forwarded but do not change status.

### Transcript fallback

`sessions.messages` prefers the live `get_messages` payload from the attached process. Without an attachment it falls back to parsing the stored JSONL transcript directly (session header plus `{"type":"message",…}` entries), so the workspace shows history even before attach.

## SSE event stream

Endpoint:

```text
GET /api/v1/events?session_id=<optional>
Accept: text/event-stream
```

Each event uses standard SSE fields:

```text
id: event-1042
event: session.updated
data: {"session_id":"session-id","state":"running"}

```

The stream sends a heartbeat comment when idle:

```text
: keepalive

```

Supported event types:

| Event | Meaning |
|---|---|
| `session.discovered` | A new session entered the catalog (watcher rescan) |
| `session.updated` | Session metadata or live state changed |
| `session.removed` | A session disappeared from the source |
| `session.attached` | A Pi RPC process is attached |
| `session.detached` | A Pi RPC process was detached or exited |
| `pi.response` | A response to a server-issued Pi command |
| `pi.event` | An asynchronous Pi agent event, forwarded verbatim |
| `error` | A recoverable stream or provider error |
| `machine.updated` | Machine metadata or reachability changed (planned) |
| `operation.updated` | A long-running action changed state (planned) |

The catalog watcher rescans the Pi session directory every two seconds and diffs it against the previous snapshot, so `session.discovered` / `session.updated` / `session.removed` fire without polling from the client.

The desktop sends `Last-Event-ID` when reconnecting. The server should replay events from a bounded in-memory/event-store buffer, then emit a `snapshot.required` event if the requested event is too old.

## Command lifecycle

```text
Desktop creates request ID
          │
          ▼
Server validates JSON-RPC + capability
          │
          ▼
Application use case creates operation ID
          │
          ▼
Server returns accepted response
          │
          ▼
Pi adapter writes JSONL command
          │
          ├── response event
          ├── progress events
          ├── agent/tool output events
          └── final operation.updated event
```

The desktop must render these states separately:

```text
created → accepted → running → succeeded
                         ├────▶ failed
                         ├────▶ cancelled
                         └────▶ unknown/requires-recovery
```

## Implementation order

1. Add `PiSessionScanner` and sanitized session fixtures.
2. Add `SessionRepository` catalog endpoints.
3. Add SSE infrastructure with heartbeats and reconnect IDs.
4. Add `PiRpcSupervisor` with one attached process.
5. Add JSON-RPC command routing and typed application use cases.
6. Add desktop SSE client and live session updates.
7. Add prompt, steer, follow-up, abort, and state controls.
8. Add authentication, pairing, capability scopes, and audit events.
