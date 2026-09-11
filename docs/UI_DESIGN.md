# Agentifi Desktop UI Direction

## Visual language

Agentifi should use a dark liquid-glass visual system rather than a flat dark theme.

The effect should be implemented as layered translucent surfaces, not as a platform-specific blur dependency:

```text
┌──────────────────────────────────────────────────────────┐
│ translucent top bar · soft highlight · connection pill  │
├───────────────┬──────────────────────────┬───────────────┤
│ frosted nav   │ glass session cards      │ glass details │
│               │                          │               │
│ machine cards │ selected card glow      │ session state  │
│               │ activity timeline        │ action rail   │
├───────────────┴──────────────────────────┴───────────────┤
│ translucent status rail · event stream · server health   │
└──────────────────────────────────────────────────────────┘
```

## Theme tokens

Use centralized tokens rather than colors embedded in widgets:

```text
background       near-black blue/violet gradient
surface          rgba(255,255,255,0.06)
surface-hover    rgba(255,255,255,0.10)
surface-selected rgba(105,190,255,0.14)
border           rgba(255,255,255,0.12)
text             cool white
muted            blue-gray
accent           cyan / electric blue
success          mint
warning          amber
error            coral
radius           16–22 px
```

## Layout regions

### Machine rail

- Machine identity and reachability
- Local/remote distinction
- Pairing state
- Server version
- Small health indicator

### Session workspace

- Search and filter controls
- Session cards or a dense table toggle
- Activity indicator
- Provider and project labels
- Streaming/idle/error state
- New-session action

### Detail panel

- Session title and project
- Current model and thinking mode
- Session state
- Resume/attach/detach controls
- Prompt input
- Steering and follow-up queue
- Abort control with confirmation

### Activity rail

- SSE connection status
- Reconnect indicator
- Pi event stream
- Tool execution progress
- Operation history

## Interaction principles

- Selecting a session is read-only until explicitly attached.
- Attach state is visible before controls become active.
- Destructive operations require confirmation.
- Streaming state disables unsafe duplicate actions.
- Every command receives visible accepted/running/completed state.
- Offline state never looks like an empty session list.
- Reconnects preserve selection and scroll position.

## Implementation sequence

1. Extract colors, spacing, radii, typography, and shadows into a desktop theme module.
2. Add background and glass surface primitives.
3. Refactor the current machine/session/details panels to use those primitives.
4. Add connection state and live-event indicators.
5. Add session activity and command controls after the protocol is implemented.
6. Add motion only for state transitions; keep accessibility and reduced-motion behavior explicit.
