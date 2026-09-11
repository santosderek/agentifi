# Agentifi Desktop Views

Agentifi uses one dark product shell with several focused views. The views are inspired by modern session-management products but use Agentifi's own domain and visual language.

## Shared shell

- Compact left navigation rail
- Top toolbar with view title, search, filters, layout toggle, connection state, and refresh
- Dark neutral surfaces with 1px low-contrast borders
- Small semantic status dots instead of neon panel outlines
- Keyboard shortcuts for search, view switching, and session actions

## Overview

The default view is a useful dashboard rather than an empty three-column workspace.

```text
┌────────────┬───────────────────────────────────────────┐
│ navigation │ Dashboard                         Refresh │
│            ├──────────┬──────────┬──────────┬──────────┤
│ search     │ sessions │ messages │ active   │ projects │
│            ├──────────┴──────────┴──────────┴──────────┤
│ recent     │ Recent sessions       Activity / projects  │
│ sessions   │                                           │
└────────────┴───────────────────────────────────────────┘
```

## Explorer

The explorer is the primary session browsing view.

- Project/session tree in the left pane
- Session list with title, summary, provider, model, message count, and last activity
- Optional branch/tree timeline in the center
- Compact preview or selected-session summary on the right

Selecting a session transitions to the session workspace without losing the current filters.

## Board

The board is an optional kanban-style view for organizing sessions by status or user-defined project columns.

Default columns:

```text
Unlabeled · Active · Paused · Needs review · Completed
```

Cards remain compact and include title, summary, project, message count, age, model, and a status dot. Dragging is a later enhancement; first release supports filtering and opening cards.

## Session workspace

The workspace is a focused conversation/control view.

```text
┌────────────┬──────────────────────────┬──────────────────┐
│ projects   │ branch/session timeline  │ conversation     │
│ and tree   │ messages/events          │ and composer     │
└────────────┴──────────────────────────┴──────────────────┘
```

Controls are context-aware:

- Prompt
- Steer
- Follow-up
- Abort
- Model selection
- Queue inspection
- Attach/detach

Pi events and tool progress appear inline in the conversation timeline and in a compact activity drawer.

## State transitions

```text
Overview ──select session──▶ Session workspace
Overview ──Explorer─────────▶ Explorer
Overview ──Board─────────────▶ Board
Explorer ──select session───▶ Session workspace
Board ────open card─────────▶ Session workspace
Workspace ──back────────────▶ previous view
```

The selected session, search query, and filters are retained while switching views.

## Data required by the UI

The session catalog must expose:

- Stable ID
- Display title
- Summary/first prompt
- Project and working directory
- Branch when available
- Created and updated timestamps
- Status and attachment state
- Message count
- Provider/model
- Source path only in diagnostics
- Tags or labels

Raw JSONL filenames and full UUIDs must not be primary display titles.

## Visual rules

- App background: dark charcoal with a slight cool tint
- Sidebar: one level lighter than the background
- Cards: subtle elevation, no bright outline
- Selected row: tinted background and a 2px accent marker
- Radius: 8–12px for dense controls and cards
- Larger 16px radius only for major workspace surfaces
- Typography hierarchy is more important than color
- Use green/orange/red/purple as small semantic accents
- Avoid large empty cards and oversized padding
