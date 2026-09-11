# Agentifi ASCII Diagrams

These diagrams describe the intended shape of the system. They are deliberately kept in the repository so the architecture can evolve alongside the implementation.

## 1. System topology

```text
┌─────────────────────────────┐
│     Agentifi Desktop        │
│                             │
│  Machines · Sessions        │
│  Details · Operations       │
└──────────────┬──────────────┘
               │ HTTP + WebSocket
               ▼
┌─────────────────────────────┐
│     Agentifi Server         │
│                             │
│  API · Pairing · Events     │
│  Authorization · Audit      │
└──────────────┬──────────────┘
               │ application ports
               ▼
┌─────────────────────────────┐
│      Application Core       │
│                             │
│  List · Search · Resume     │
│  Stop · Pair · Subscribe    │
└──────────────┬──────────────┘
               │ domain ports
      ┌────────┼─────────┬────────────┐
      ▼        ▼         ▼            ▼
┌──────────┐ ┌───────┐ ┌──────────┐ ┌────────────┐
│ Pi source│ │ SQLite│ │ Process  │ │ Event bus  │
│ adapter  │ │ cache │ │ control  │ │ adapter    │
└──────────┘ └───────┘ └──────────┘ └────────────┘
```

## 2. Local desktop startup

```text
┌────────────────────┐
│ Desktop starts     │
└─────────┬──────────┘
          ▼
┌────────────────────┐       yes      ┌────────────────────┐
│ GET /api/v1/...    │───────────────▶│ Use existing server │
│ on 127.0.0.1:8787  │                └─────────┬──────────┘
└─────────┬──────────┘                          ▼
          │ no                         ┌────────────────────┐
          ▼                            │ Load sessions     │
┌────────────────────┐                  └────────────────────┘
│ Spawn server child │
└─────────┬──────────┘
          ▼
┌────────────────────┐
│ Poll readiness     │
└─────────┬──────────┘
          ▼
┌────────────────────┐
│ Connect desktop    │
└─────────┬──────────┘
          ▼
┌────────────────────┐
│ Own child lifetime │
│ until app exits    │
└────────────────────┘
```

## 3. Request flow

```text
┌──────────────┐
│ egui button  │
└──────┬───────┘
       ▼
┌──────────────┐       ┌────────────────┐
│ Client API   │──────▶│ HTTP adapter   │
└──────────────┘       └───────┬────────┘
                               ▼
                       ┌────────────────┐
                       │ Use case       │
                       │ ResumeSession  │
                       └───────┬────────┘
                               ▼
                       ┌────────────────┐
                       │ Domain policy  │
                       └───────┬────────┘
                               ▼
                       ┌────────────────┐
                       │ Controller port│
                       └───────┬────────┘
                               ▼
                       ┌────────────────┐
                       │ Pi/process     │
                       │ adapter        │
                       └────────────────┘
```

## 4. Desktop composition

```text
┌──────────────────────────────────────────────────────────┐
│ Agentifi                         Refresh   Connection    │
├───────────────┬──────────────────────────┬───────────────┤
│ Machines      │ Sessions                 │ Details       │
│               │                          │               │
│ ● This        │ ┌──────────────────────┐ │ Session title │
│   machine     │ │ Session A · active  │ │ Project       │
│               │ ├──────────────────────┤ │ Status        │
│ Future:       │ │ Session B · paused  │ │               │
│ remote        │ └──────────────────────┘ │ Resume  Stop  │
│ machines      │                          │               │
├───────────────┴──────────────────────────┴───────────────┤
│ Connected · 2 sessions                     127.0.0.1:8787│
└──────────────────────────────────────────────────────────┘
```

## 5. Hexagonal boundary

```text
                  inbound adapters
          ┌────────────┬─────────────┐
          │ HTTP       │ WebSocket   │ Desktop client
          └──────┬─────┴──────┬──────┘
                 ▼            ▼
             ┌────────────────────┐
             │ Application        │
             │ use cases          │
             └─────────┬──────────┘
                       ▼
             ┌────────────────────┐
             │ Domain             │
             │ entities + rules   │
             └─────────┬──────────┘
                       ▼
             ┌────────────────────┐
             │ Outbound ports     │
             └──┬────────┬──────┬─┘
                ▼        ▼      ▼
             Pi source  Storage  Process
             adapter    adapter  adapter
```
