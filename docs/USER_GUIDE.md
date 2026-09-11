# Agentifi User Guide (Planned)

## Install the server

Install the server on the machine where Pi sessions run. Start it in local-only mode first:

```bash
agentifi-server
```

The default health endpoint is `http://127.0.0.1:8787/health`. Do not bind it to a public interface until pairing and transport security are configured.

When the desktop client starts, it first checks the local server. If one is already running, it connects to that instance without starting another process. If no local server responds, it starts the colocated `agentifi-server` executable and waits for readiness. Set `AGENTIFI_SERVER_COMMAND` when the executable is installed elsewhere. The desktop-owned local server is stopped when the desktop client exits.

## Install the desktop client

Install the desktop application on the operator machine. The client connects to a server URL and stores its paired device credential in the operating system credential store.

## Pair a machine

1. Start the server locally.
2. Open the desktop client and choose **Add machine**.
3. Enter the server URL or scan the one-time pairing code.
4. Approve the pairing on the server machine.
5. Confirm the machine identity and granted capabilities.

Pairing is intentionally explicit. A server must never accept an unknown desktop client silently.

## Browse sessions

The desktop client groups sessions by machine and project. Each session displays its source, current state, last observed time, and whether the data is live or cached.

## Control a session

Actions such as resume, pause, stop, export, and terminate display their required capability and confirmation. Long-running actions show progress and a request ID that can be used to recover after reconnecting.

## Troubleshooting

- If the server is unreachable, check the server health endpoint and bind address.
- If sessions are missing, check the provider adapter status and its read permissions.
- If an action is pending, reconnect and inspect the operation ID before repeating it.
- If the desktop shows stale data, use refresh and inspect the observed timestamp.
- Never solve a connection problem by disabling authentication on a remotely bound server.
