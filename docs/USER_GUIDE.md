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

The desktop client groups sessions by project. Each session displays a readable title
(taken from the transcript's name or first prompt, never a UUID or JSONL filename), a
summary, its project, model, message and tool-call counts, and how long ago it was
active. The Explorer adds search, status/project/model filters, and sorting; the Board
arranges the same sessions into Inbox, Active, Paused, Needs review, and Completed lanes.
Lanes are your own organisational state and never change what Pi is doing.

## Keyboard shortcuts

| Keys | Action |
| --- | --- |
| `g o` / `g e` / `g b` | Overview / Explorer / Board |
| `/` | Jump to Explorer search |
| `1`–`9` | Open the Nth most recent session |
| `Enter` | Open the selected session |
| `Esc` | Leave the workspace, or clear filters |
| `[` / `]` | Move the selected board card between lanes |
| `Ctrl+Enter` | Send the prompt (queues a follow-up while Pi is working) |
| `Ctrl+S` | Steer the running turn |
| `F5` | Refresh the session catalog |

## Control a session

Open a session to reach its workspace: context on the left, the live transcript in the
middle, the inspector on the right, and the prompt composer at the bottom. The
composer's primary action follows the session's real state — attach, send, queue a
follow-up, steer, or resume a finished transcript — so the visible button is always the
action that will actually happen. Abort is available only while Pi is working and always
asks for confirmation. Command results and connection changes appear as toasts, and the
status rail shows whether the event stream is live.

## Troubleshooting

- If the server is unreachable, check the server health endpoint and bind address.
- If sessions are missing, check the provider adapter status and its read permissions.
- If an action is pending, reconnect and inspect the operation ID before repeating it.
- If the desktop shows stale data, use refresh and inspect the observed timestamp.
- Never solve a connection problem by disabling authentication on a remotely bound server.
