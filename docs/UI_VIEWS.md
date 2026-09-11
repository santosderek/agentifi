# Agentifi Desktop Views

Four views share one shell. The shell is a left navigation rail, a bottom status rail,
a toast layer, and a central content area that each view owns. Visual rules live in
[UI_DESIGN.md](UI_DESIGN.md); this document describes what each view shows and what the
user can do there.

## Shared shell

```text
┌──────────────┬──────────────────────────────────────────────────────┐
│ agentifi     │  view title                                          │
│              │  subtitle · search · filters · sort · refresh        │
│ Overview g o │ ──────────────────────────────────────────────────── │
│ Explorer g e │                                                      │
│ Board    g b │   view content                                       │
│              │                                                      │
│ Projects     │                                            ┌───────┐ │
│  agentifi 12 │                                            │ toast │ │
│  pi-core   4 │                                            └───────┘ │
│ ● live       │                                                      │
├──────────────┴──────────────────────────────────────────────────────┤
│ Explorer · 33 sessions · 127.0.0.1:8787            ● stream live    │
└─────────────────────────────────────────────────────────────────────┘
```

The rail carries the three primary views with their shortcut hints, the projects with
session counts, and the live connection state. The status rail carries the current view,
the catalog summary, and whether the SSE stream is connected.

## Overview

The Overview is deliberately quiet, modelled on a terminal dashboard: a single 760px
column, generous vertical rhythm, a monospace greeting, hairline rules instead of cards,
and a key hint beside every action.

```text
        Good evening.
        33 sessions  ·  6 projects  ·  connected to the local Pi server

        33 sessions   4 active   1 need review   1842 messages
        ─────────────────────────────────────────────────────────

        Resume most recent session                                r
        Browse all sessions                                       e
        Open board                                                b
        Refresh catalog                                          F5

        RECENT SESSIONS ─────────────────────────────────────────
        Refactor the adapter cache            agentifi   12m   1
        Fix flaky SSE reconnect test          pi-core     1h   2
        …

        LIVE ACTIVITY ───────────────────────────────────────────
        22:41  tool     read_file crates/domain/src/lib.rs
        22:41  agent    Updated the lane mapping and its tests
```

- The stat strip is borderless text; there are no metric tiles.
- Digits `1`–`9` open the corresponding recent session.
- The footer restates the global shortcuts, so the view teaches itself.

## Explorer

The durable inventory: project tree on the left, dense session list in the middle,
optional inspector on the right.

```text
┌───────────┬─────────────────────────────────────────┬──────────────┐
│ PROJECTS  │ ● Refactor the adapter cache            │ Selected     │
│ All   33  │   first prompt summary…                 │ Details      │
│ agentifi  │   agentifi · sonnet · 42 msgs · 12 tools│ Activity     │
│ pi-core   │ ─────────────────────────────────────── │ Files        │
│ STATUS    │ ○ Fix flaky SSE reconnect test          │ Diagnostics  │
│ active 4  │   …                                     │              │
└───────────┴─────────────────────────────────────────┴──────────────┘
```

- Search covers titles, first prompts, projects, branches, models, and tags.
- Filter chips show their current value (`Status: active`) and open a menu in place.
- Sort by last active, title, or message count.
- Click selects and opens the inspector; double click or `Enter` opens the workspace;
  right click gives open, filter-to-project, and copy-ID.
- The empty state offers the action that matches the cause: clear filters when filters
  are active, refresh otherwise.

## Board

Operational triage across five lanes: Inbox, Active, Paused, Needs review, Completed.

```text
┌ Inbox 3 ─┬ Active 4 ─┬ Paused 1 ─┬ Needs review 2 ┬ Completed 23 ┐
│ ┌──────┐ │ ┌───────┐ │           │ ┌────────────┐ │              │
│ │ card │ │ │ card  │ │           │ │ card       │ │              │
│ └──────┘ │ └───────┘ │           │ └────────────┘ │              │
```

- A lane is the operator's own state. The card's status dot keeps reporting what Pi is
  actually doing, so the two never get conflated.
- `[` and `]` move the selected card between lanes; the right-click menu offers the
  same move explicitly. Drag and drop is deliberately deferred.
- Clicking a lane heading narrows the Explorer to the matching status.
- Double click opens the workspace.

## Session workspace

Where the work happens: context rail, transcript, inspector, composer.

```text
Explorer / agentifi / Refactor the adapter cache
← Refactor the adapter cache                    Reattach · Inspector
attached · active · 12m
┌────────────┬──────────────────────────────┬────────────────────┐
│ CONTEXT    │ TRANSCRIPT     all msgs tools│ INSPECTOR          │
│ Project    │ 22:40 user   Refactor the …  │ Details            │
│ Branch     │ 22:40 tool   read_file …     │ Status   active    │
│ Directory  │ 22:41 agent  Updated the …   │ Model    sonnet    │
│ Model      │                              │ Messages 42        │
│ SESSIONS   │                              │                    │
└────────────┴──────────────────────────────┴────────────────────┘
┌ Pi is working. Steer interrupts immediately; a follow-up is queued ┐
│ [ prompt …                                    ] Queue follow-up   │
│                                                 Steer   Abort     │
└───────────────────────────────────────────────────────────────────┘
Ctrl+Enter send · Ctrl+S steer · Esc back
```

- The composer's primary action follows real session state: `Attach session` when
  detached, `Resume session` when the transcript is finished, `Queue follow-up` plus
  `Steer` while Pi is working, and `Send` when idle and attached.
- Abort is enabled only while Pi is working and always requires a second confirm.
- The transcript is one stream — prompts, agent output, tool calls, and errors — with
  filters for messages or tools only.
- The context rail lists sibling sessions in the same project so switching does not
  require going back to the Explorer.

## Deferred on purpose

Drag-and-drop board reordering, cost and token analytics, cross-session search, and
multi-machine grouping are all out of scope for this pass. Each needs data the server
does not report yet, and shipping a placeholder would violate the "no decoration
without information" rule.
