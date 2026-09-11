# Agentifi Desktop Design System

Agentifi's desktop client is a calm technical console. It is a tool a developer keeps
open all day next to a terminal and an editor, so it optimises for legibility,
density where density helps, and silence everywhere else. There is no glass, no glow,
no gradient, and no decoration that does not carry information.

The implementation of everything below lives in `apps/agentifi-desktop/src/theme.rs`
and `apps/agentifi-desktop/src/components/`. The theme is installed once at startup
(`theme::install`), never per frame.

## Principles

1. **Typography over colour.** Hierarchy comes from size, weight, and whitespace.
   Colour is reserved for meaning.
2. **One hairline per surface.** A surface gets a single 1px low-contrast border. No
   double borders, no outer glow, no shadow stacking.
3. **Semantic colour is small.** Status appears as a 6px dot, a 2px left accent
   strip, or a small monospace label. Never as a filled panel or a coloured button.
4. **Selection is a shift in surface, not an outline.** A selected row uses a
   slightly lighter surface plus a 2px accent strip on the left edge.
5. **Empty states are written, not blank.** Every empty region explains what will
   appear there and, where an action makes sense, offers exactly one.
6. **Identifiers are diagnostics.** UUIDs, JSONL filenames, and file paths never act
   as a title. They live in the inspector's `Diagnostics` tab.
7. **The keyboard is a first-class input.** Anything reachable by mouse in a list or
   lane is reachable by key, and the key is printed next to the action.

## Colour

| Token | Value | Use |
| --- | --- | --- |
| `APP_BG` | `#171821` | Application background |
| `SIDEBAR_BG` | `#1B1C27` | Navigation rail, status rail |
| `SURFACE` | `#1D1E29` | Cards, panels |
| `SURFACE_ELEVATED` | `#22232F` | Hover, toasts |
| `SURFACE_HOVER` | `#292A38` | Pressed and dense hover |
| `SURFACE_SELECTED` | `#262938` | Selected row or card |
| `CODE_BG` | `#12131B` | Transcript and code blocks |
| `BORDER` | `#2B2D3A` | Primary hairline |
| `BORDER_SUBTLE` | `#232530` | Row separators |
| `TEXT_PRIMARY` | `#F1F1F5` | Titles, prompts |
| `TEXT_SECONDARY` | `#8B8E9F` | Body, values |
| `TEXT_MUTED` | `#626A79` | Labels, metadata, key hints |

Semantic colours: blue `#56A8E8` (idle, connection), green `#6ED18B` (active,
attached), orange `#F3A45B` (paused, warning), purple `#B982E8` (needs review, tool
calls), red `#F06C73` (failed, error).

Status and lane colour mapping is centralised in `theme::status_color` and the board's
lane palette, so a status never gets two different colours in two views.

## Type scale

Inter for prose, JetBrains Mono for metadata, identifiers, timestamps, and key hints.
Monospace is a signal that the value is machine data.

| Role | Size | Family |
| --- | --- | --- |
| Display (Overview greeting) | 26 | Mono |
| Title (view heading) | 17 | Sans |
| Row title | 14 | Sans |
| Body | 13 | Sans |
| Meta | 11.5 | Mono |
| Label / key hint | 10.5 | Mono |

## Spacing, radius, density

- Spacing scale: 4, 8, 12, 18, 28, 44.
- Radii: 7px for controls, 10px for cards. Nothing is fully rounded, nothing is sharp.
- Row padding 8–12px; card padding 12–16px.
- The Overview constrains content to a 760px column and lets the rest be whitespace.
- Long values are truncated at the pixel boundary with a hover tooltip carrying the
  full string, never wrapped mid-word.

## Component contract

| Component | Responsibility |
| --- | --- |
| `text` | Truncating single-line paint, dots, meta joins, section labels, key hints |
| `session_row` | One session in `Full`, `Compact`, or `Minimal` density |
| `board_card` | One session as a lane card |
| `status_badge` | Status dot and the workspace header summary string |
| `navigation_rail` | Nav items with shortcut hints, project shortcuts, connection state |
| `toolbar` | View title block, search field, filter menus, breadcrumb, counts |
| `stat` | Borderless value + label used by the minimal Overview |
| `activity_timeline` | Event stream, filterable by messages or tools |
| `prompt_composer` | State-aware prompt entry: attach, send, follow-up, steer, abort |
| `inspector` | Details, Activity, Files, Diagnostics tabs |
| `empty_state` | Written empty state with at most one action |
| `toast` | Transient command and connection feedback |
| `icon` | Hand-drawn line icons; no icon font dependency |

Components never perform I/O. Views read a `ViewContext` and return `Action`s, and the
shell (`app.rs`) is the only place that talks to the server.

## Interaction rules

- Single click selects; double click opens the workspace; `Enter` opens the selection.
- Navigation uses a `g` prefix: `g o`, `g e`, `g b`. `/` jumps to Explorer search.
  `Esc` closes the workspace, or clears filters when no workspace is open.
- `1`–`9` open the Nth most recent session from anywhere.
- `Ctrl+Enter` sends (queues a follow-up when Pi is working), `Ctrl+S` steers.
- Abort always requires an explicit confirm step and is only enabled while Pi works.
- Filters, selection, and sort survive navigation between views.
- Board lanes are the operator's organisational state and never mutate the Pi process.
