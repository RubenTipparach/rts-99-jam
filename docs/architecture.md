# Design & Architecture — Web-First StarCraft-Style RTS

> Status: **Draft v0.1** · Working title: `rts-99-jam` · Target: browser-first (WASM/WebGPU) + desktop (native)
>
> This document surveys the technologies and algorithms needed to build a
> StarCraft-style multiplayer RTS in Rust, and proposes a concrete architecture.
> It is opinionated where a decision is needed and honest where a requirement is
> genuinely hard. Every claim links to a primary reference; a consolidated
> reference list lives at the [end](#references).

---

## Table of contents

1. [Vision & design pillars](#1-vision--design-pillars)
2. [Scope reality check (read this first)](#2-scope-reality-check-read-this-first)
3. [The central decision: engine substrate](#3-the-central-decision-engine-substrate)
4. [High-level architecture](#4-high-level-architecture)
5. [The deterministic simulation core](#5-the-deterministic-simulation-core)
6. [Multiplayer netcode](#6-multiplayer-netcode)
7. [Rendering & the 120–160 fps budget](#7-rendering--the-120160-fps-budget)
8. [Units at scale (100s–1000s)](#8-units-at-scale-100s1000s)
9. [Animation](#9-animation)
10. [Particles & VFX](#10-particles--vfx)
11. [Lighting](#11-lighting)
12. [Large maps, planets & seamless space transitions](#12-large-maps-planets--seamless-space-transitions)
13. [Movement & pathfinding](#13-movement--pathfinding)
14. [AI bots (first-class, built alongside)](#14-ai-bots-first-class-built-alongside)
15. [Web target constraints](#15-web-target-constraints)
16. [Proposed Cargo workspace layout](#16-proposed-cargo-workspace-layout)
17. [Consolidated technology stack](#17-consolidated-technology-stack)
18. [Phased roadmap](#18-phased-roadmap)
19. [Risk register & open questions](#19-risk-register--open-questions)
20. [References](#references)

---

## 1. Vision & design pillars

A real-time strategy game in the lineage of StarCraft / Supreme Commander /
Planetary Annihilation, with these non-negotiable pillars:

| Pillar | Implication |
| --- | --- |
| **Web-first, desktop-second** | Ship to the browser via WASM + WebGPU; native desktop is the same codebase compiled differently. This constrains threading, binary size, and the graphics feature set. |
| **Competitive multiplayer** | Determinism + fairness + low bandwidth. This is what pushes us to lockstep. |
| **Massive unit counts (100s–1000s)** | Data-oriented simulation, flow-field movement, GPU-instanced rendering. |
| **High frame rate (120–160 fps)** | Decouple simulation tick rate from render rate; interpolate. |
| **Planetary scale + seamless space** | Floating-origin world, LOD streaming, cube-sphere planets. |
| **AI bots from day one** | Bots are not bolted on; they issue the same command stream as players and run headless against the sim core. |

---

## 2. Scope reality check (read this first)

Be clear-eyed: the requested feature list — deterministic 1000-unit lockstep
netcode, procedural planets, seamless planet↔space transitions, spherical
pathfinding, 3D space pathfinding, GPU crowd rendering, animation, AI — is, in
aggregate, **the scope of a funded studio building a custom engine over multiple
years**. Several individual items (cross-platform deterministic simulation;
seamless space-to-surface; spherical navigation that coexists with flow fields)
are each meaty research/engineering problems.

This is not a reason to abandon the vision. It is a reason to architect so that
the hard pieces are **isolated, swappable, and sequenced**. The architecture
below deliberately:

- Puts a thin, replaceable abstraction in front of every hard subsystem
  (pathfinding, transport, planet LOD) so an MVP stub can ship before the
  research-grade version.
- Phases the roadmap so there is a *playable* game (flat map, single battlefield,
  no planets) long before there is an *impressive* one.

See [§18 Phased roadmap](#18-phased-roadmap). Treat planets and seamless space as
**Phase 3+**; treat lockstep + flow-field combat on a flat map as the **MVP**.

---

## 3. The central decision: engine substrate

You proposed *Rust + wgpu + a Rust UI crate*. That is exactly right — but note
that the dominant Rust engine, **[Bevy](https://bevy.org/)**, *is* "wgpu + an ECS
+ an ecosystem," and most of the crates you need (GPU particles, floating origin,
netcode) are built for Bevy. So the real choice is:

| Option | What you get | Cost |
| --- | --- | --- |
| **A. Bevy as substrate** (recommended) | wgpu renderer, mature ECS that "scales to millions of entities," `bevy_ui`, asset pipeline, and a networking/particle/space ecosystem (`bevy_hanabi`, `big_space`, `lightyear`). Targets native + WASM/WebGPU from one codebase. | You inherit Bevy's architecture and upgrade churn; its parallel ECS is **not** designed for cross-platform determinism, so you must keep the sim core *outside* of it. |
| **B. Custom wgpu engine** | Total control over the render loop, memory, and determinism; smallest WASM binary. | You rebuild ECS, asset loading, UI integration, animation, and every ecosystem crate yourself. Months of plumbing before a triangle becomes a game. |
| **C. Hybrid (the actual recommendation)** | A **standalone deterministic sim crate** (pure Rust, no engine deps) + **Bevy for everything else** (rendering, input, UI, assets, audio). | A clean seam to maintain, but it is the *right* seam — and it is the same seam custom-engine RTS teams converge on. |

**Recommendation: Option C.** Build the deterministic simulation as an
engine-agnostic crate. Use Bevy (on wgpu) as the presentation/IO host. The sim
crate also compiles standalone for the dedicated server and for headless AI
training/CI. This is the StarCraft/AoE architecture: a deterministic command-driven
simulation with a separate rendering front-end.

> Why not just use Bevy's ECS for the simulation too? Because Bevy's scheduler runs
> systems in parallel with non-deterministic ordering, and floating-point results
> can differ across platforms (native vs. WASM, x86 vs. ARM). Lockstep requires
> **bit-identical** simulation on every machine. See
> [§5](#5-the-deterministic-simulation-core) and Bevy's own
> ["Auditing determinism" discussion](https://github.com/bevyengine/bevy/discussions/2480).

**UI crate:** for an in-game RTS HUD and especially a map/unit editor, **[egui](https://github.com/emilk/egui)**
(immediate-mode, tiny WASM binary, trivial wgpu/Bevy integration via `bevy_egui`)
is the pragmatic default; **[iced](https://iced.rs)** (retained, Elm-style) is the
alternative if you want a more "application-like" retained UI. egui wins for
overlays, debug tooling, and editors; see the
[2025 comparison](https://an4t.com/rust-gui-libraries-compared/) and
[community threads](https://users.rust-lang.org/t/egui-vs-iced-in-regards-to-game-engine-integration/74569).
For the polished shipping HUD you may end up drawing custom widgets in `bevy_ui`;
keep egui for tools regardless.

---

## 4. High-level architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                         CLIENT (WASM / native)                         │
│                                                                        │
│  ┌────────────────┐   render view    ┌──────────────────────────────┐  │
│  │  PRESENTATION  │◄─────(read-only)──│   DETERMINISTIC SIM CORE     │  │
│  │  (Bevy + wgpu) │   interpolated    │   (engine-agnostic crate)    │  │
│  │                │                   │                              │  │
│  │ • render @120+ │   local commands  │ • fixed-point math           │  │
│  │ • input/UI     │──────────────────►│ • fixed tick @ 20–30 Hz      │  │
│  │ • egui tools   │                   │ • command-driven             │  │
│  │ • audio        │                   │ • bit-identical everywhere   │  │
│  └────────────────┘                   │ • produces checksums         │  │
│         ▲                             └──────────────┬───────────────┘  │
│         │                                            │ command stream   │
│  ┌──────┴───────┐                            ┌───────┴───────────────┐  │
│  │  AI MODULE   │── commands ───────────────►│   NETCODE / LOCKSTEP  │  │
│  │ (same cmd API│                            │   (input relay)       │  │
│  │  as a human) │                            └───────┬───────────────┘  │
│  └──────────────┘                                    │                  │
└──────────────────────────────────────────────────────┼──────────────────┘
                                                        │ inputs + checksums
                          WebRTC DataChannel / WebTransport / WS
                                                        │
                               ┌────────────────────────┴───────────────┐
                               │   RELAY / SESSION SERVER (server-assist)│
                               │ • signaling + matchmaking               │
                               │ • input relay (star topology)           │
                               │ • turn/lockstep coordination            │
                               │ • checksum/desync + anti-cheat           │
                               │ • replay log, reconnect, late-join snap │
                               │ • optional headless authoritative sim   │
                               └─────────────────────────────────────────┘
```

**The one rule that makes everything work:** the presentation layer only ever
*reads* simulation state and renders an interpolated snapshot; it never mutates
game state directly. All state changes flow through **commands** → sim core.
Humans, AI, network peers, and replays are all just **command sources**.

---

## 5. The deterministic simulation core

This is the heart. Get it right and lockstep netcode, replays, server validation,
and headless AI all fall out almost for free. Get it wrong and you fight desyncs
forever.

### 5.1 Determinism strategy

Lockstep transmits only player inputs; every client re-simulates the entire game
from those inputs and **must** arrive at identical state. Any divergence ("desync")
is fatal. The classic references: Glenn Fiedler's
[Deterministic Lockstep](https://gafferongames.com/post/deterministic_lockstep/) and
[Floating Point Determinism](https://gafferongames.com/post/floating_point_determinism/),
and the canonical
[cross-platform RTS synchronization & floating-point indeterminism](https://www.gamedeveloper.com/programming/cross-platform-rts-synchronization-and-floating-point-indeterminism)
write-up.

Two viable routes to bit-identical math:

1. **Fixed-point arithmetic (recommended for cross-platform).** All gameplay math
   uses integers under the hood — positions, velocities, health — via the
   [`fixed`](https://crates.io/crates/fixed) crate, with trig/`sqrt` implemented
   through CORDIC (e.g. the [`cordic`](https://crates.io/crates/cordic) crate).
   This is exactly the approach Rapier/nphysics took to get cross-platform
   determinism — see the [Rapier determinism guide](https://rapier.rs/docs/user_guides/rust/determinism/).
   Integers are deterministic by construction.
2. **Carefully-controlled `f32`/`f64`.** Per Fiedler and Rapier's
   `enhanced-determinism`, IEEE-754 can be made cross-platform deterministic *if*
   you forbid fused-multiply-add, use a fixed `libm` (not hardware intrinsics),
   and avoid SIMD paths whose reductions reorder. This is workable —
   ["target platforms strictly complying with IEEE-754-2008 … including WASM"](https://rapier.rs/docs/user_guides/rust/determinism/)
   — but it is fragile and easy to regress. **Do not** use SIMD `glam` types in the
   sim core for this reason; `glam` is fine in the *render* layer.

> **Recommendation:** fixed-point. It is the only option that is deterministic by
> default rather than by perpetual vigilance, and it makes the WASM-vs-native
> parity (your web-first requirement) a non-issue.

Determinism also demands:
- **Deterministic iteration order.** No `HashMap` iteration in sim logic (use
  sorted/`IndexMap`-style or dense arrays keyed by entity id). This alone rules
  out naively running the sim inside Bevy's parallel ECS.
- **A single fixed timestep.** Simulate at e.g. **20–30 Hz** (StarCraft ran its
  "game turns" far slower than its frame rate). All randomness comes from a
  seeded, deterministic PRNG advanced in lockstep.
- **No wall-clock, no `f32` time deltas** inside the sim. Tick counts only.

### 5.2 Simulation data model

A compact, data-oriented store — even if you adopt Bevy elsewhere, the sim core
should use its own tight ECS or struct-of-arrays. Options:
[`hecs`](https://crates.io/crates/hecs) (minimal, fast, engine-agnostic),
plain SoA arrays, or `bevy_ecs` used *standalone and single-threaded with a fixed
system order*. Community RTS-in-ECS design notes:
[Bevy RTS discussion #2659](https://github.com/bevyengine/bevy/discussions/2659).

### 5.3 What you get for free

Because the sim is deterministic and command-driven:
- **Replays** = the seed + the command log. Tiny files; perfect fidelity.
- **Server validation / anti-cheat** = re-run the same sim server-side and compare
  checksums.
- **Headless AI & CI** = run thousands of games per second with no renderer to
  test balance and catch desyncs.
- **Reconnection / late-join** = ship a full state snapshot + subsequent commands.

---

## 6. Multiplayer netcode

### 6.1 Model: server-assisted deterministic lockstep

For an RTS with thousands of units, **state replication is the wrong tool** — there
is simply too much state to stream every tick (the reason every classic RTS uses
lockstep is stated plainly in
[Fiedler's deterministic-lockstep article](https://gafferongames.com/post/deterministic_lockstep/)).
Instead, transmit only **commands** (move/attack/build orders), which are tiny and
constant regardless of army size.

You asked for "P2P with server assist" — that is precisely the right shape. Use a
**star topology** lockstep: peers send inputs to a lightweight session server that
relays them to all participants and coordinates the lockstep turn schedule. The
server "assists" by:

- **Signaling & matchmaking** (required to establish WebRTC anyway).
- **Input relay** so you avoid an N×N P2P mesh and sidestep most NAT-traversal pain.
- **Turn coordination & input-delay**: lockstep advances only when all players'
  inputs for a turn have arrived; the server manages the input-delay window and the
  stall when someone lags (the classic "waiting for players…" moment).
- **Checksum/desync detection**: every client hashes its state each turn and sends
  the hash; the server flags divergence.
- **Replay logging, reconnection, late-join snapshots.**
- **Optional authoritative headless sim** for anti-cheat and spectators.

A balanced primer on the transport trade-offs for browser games:
[WebRTC vs WebSockets for multiplayer games](https://developers.rune.ai/blog/webrtc-vs-websockets-for-multiplayer-games).

> **Anti-cheat caveat (be honest about this):** pure lockstep means every client
> simulates the *entire* game, so each client holds full map state — making
> "maphacks" (seeing through fog of war) possible, since fog is enforced only in
> the client's renderer. Mitigation is an authoritative server sim that sends each
> client only what it can see, which sacrifices lockstep's bandwidth win. Most RTS
> ship lockstep and accept the maphack risk; decide consciously.

### 6.2 Transport (web-first reality)

You need UDP-like unreliable/unordered messaging, which the browser does not expose
directly. Two routes, both supported in Rust:

| Transport | Browser support | Use it for | Rust |
| --- | --- | --- | --- |
| **WebRTC DataChannel** | Everywhere today; P2P-capable | The proven path for low-latency browser games; pairs naturally with P2P/relay | **[Matchbox](https://github.com/johanhelsing/matchbox)** — "painless P2P WebRTC for Rust wasm *and* native," with a signaling server crate and multiple configurable-reliability channels ([0.6 notes](https://johanhelsing.studio/posts/matchbox-0-6)) |
| **WebTransport** (HTTP/3 / QUIC) | Modern browsers (rising fast; ~95% of major browsers support HTTP/3 per [late-2025 data](https://dev.to/instatunnel/beyond-http-exposing-webrtc-and-local-game-servers-via-udp-tunnels-5ak5)) | Client↔server datagrams without WebRTC's ICE/STUN/TURN machinery; cleaner for a star topology | `wtransport`/`aeronet`-style stacks; see [WebCodecs/WebTransport/WebRTC](https://webrtchacks.com/webcodecs-webtransport-and-webrtc/) |
| **WebSocket** (TCP) | Universal | Signaling, lobby, fallback. Avoid for in-game state — TCP head-of-line blocking hurts | `tokio-tungstenite`, `axum` |

**Recommendation:** **Matchbox** for the realtime channel (it was literally built
to bring rollback-style netcode to browsers and is battle-tested by the Bevy
community), with a WebSocket signaling/lobby server. Keep a transport trait so you
can add WebTransport later as it matures. Native desktop can use plain UDP or QUIC
([`quinn`](https://crates.io/crates/quinn)) behind the same trait.

### 6.3 Lockstep vs. rollback — and the Bevy ecosystem

Two deterministic-netcode families exist in Rust, both P2P-friendly via Matchbox:

- **Lockstep** (wait for all inputs; add input delay). Best for RTS: scales to huge
  unit counts, low bandwidth, simple. Latency is hidden by input delay, which is
  acceptable for strategy games.
- **Rollback** ([GGRS](https://github.com/gschup/ggrs), a Rust GGPO reimagining;
  see the [Extreme Bevy](https://johanhelsing.studio/posts/extreme-bevy) tutorial
  and ["Rock and Rollback"](https://www.metabrew.com/article/rock-and-rollback-realtime-multiplayer-games-with-bevy)).
  Predicts and re-simulates on misprediction — superb for fighting games, but
  re-simulating 1000s of units every rollback frame is expensive. Use rollback only
  if you find input delay unacceptable; for an RTS, **lockstep is the default**.

For a Bevy-integrated higher-level option, **[Lightyear](https://github.com/cBournhonesque/lightyear)**
supports both server-authoritative replication *and* deterministic
input-only replication "compatible with both lockstep and prediction/rollback"
([docs](https://docs.rs/lightyear/latest/lightyear/)). It is worth evaluating, but
for a deterministic lockstep RTS you may prefer to own the lockstep loop directly
on top of Matchbox so the sim core stays engine-agnostic. Other references:
[renet](https://github.com/lucaspoffo/renet),
[bevy_quinnet](https://github.com/Henauxg/bevy_quinnet), and the
[netcode protocol](https://github.com/benny-n/netcode) (Fiedler's UDP
connection protocol) for native.

---

## 7. Rendering & the 120–160 fps budget

### 7.1 The budget forces sim/render decoupling

160 fps = **6.25 ms/frame**; 144 fps ≈ 6.9 ms; 120 fps ≈ 8.3 ms. That is the entire
CPU+GPU budget. You cannot afford to run a 1000-unit simulation step inside that
budget every frame — and you don't need to. Run the **sim at 20–30 Hz** (33–50 ms
between ticks) on a fixed timestep, and have the renderer **interpolate** unit
transforms between the last two sim snapshots every display frame. This is standard
"fixed timestep, interpolated rendering."

This decoupling is *also* what makes high frame rate compatible with determinism:
the deterministic part is slow and exact; the smooth part is fast and approximate.

### 7.2 wgpu foundation

[**wgpu**](https://wgpu.rs/) is the right base: a pure-Rust, cross-platform graphics
API targeting Vulkan/Metal/DX12/GL natively and **WebGPU + WebGL2 in the browser**
([repo](https://github.com/gfx-rs/wgpu)). WebGPU is now shipping in all major
browsers ([web.dev, 2025](https://web.dev/blog/webgpu-supported-major-browsers)),
with WebGL2 as the fallback for stragglers. Pair with
[`winit`](https://crates.io/crates/winit) for windowing
([learn-wgpu](https://sotrh.github.io/learn-wgpu/)). Bevy gives you all of this
pre-wired; if you go custom, see
[building browser games with WASM/WebGPU/Rust](https://techbytes.app/posts/build-browser-games-wasm-webgpu-rust-2026/).

### 7.3 GPU-driven rendering for crowds

The way you draw 1000s of units at 144 fps is **GPU instancing + indirect draws +
GPU culling/LOD** — one draw call per mesh/material covering thousands of instances,
with per-instance transforms in a storage buffer, culling and LOD selection done on
the GPU via `draw_indirect`. Background:
[vkguide GPU-driven rendering](https://vkguide.dev/docs/gpudriven/gpu_driven_engines/),
[GPU-driven instancing](https://playerunknownproductions.net/news/gpu-driven-instancing).
For the farthest units, swap meshes for **impostors** (billboarded pre-rendered
sprites) — the classic crowd-rendering technique
([impostors & pseudo-instancing for GPU crowd rendering](https://www.researchgate.net/publication/220979001_Impostors_and_pseudo-instancing_for_GPU_crowd_rendering)).
A pure-Rust GPU-driven reference renderer on wgpu:
[`renderling`](https://github.com/schell/renderling).

---

## 8. Units at scale (100s–1000s)

Three independent scaling problems, three different solutions:

| Problem | Solution | Reference |
| --- | --- | --- |
| **Simulating** thousands of entities | Data-oriented ECS / SoA; sim at 20–30 Hz, not per-frame | Bevy ECS "scales to millions of entities"; [RTS-in-ECS design](https://github.com/bevyengine/bevy/discussions/2659) |
| **Drawing** thousands of entities | GPU instancing + indirect + LOD/impostors (see [§7.3](#73-gpu-driven-rendering-for-crowds)) | [vkguide](https://vkguide.dev/docs/gpudriven/gpu_driven_engines/) |
| **Moving** thousands of entities | Flow fields (shared path) + local steering/ORCA (see [§13](#13-movement--pathfinding)) | [Crowd Pathfinding & Steering Using Flow-Field Tiles (Game AI Pro)](https://www.gameaipro.com/GameAIPro/GameAIPro_Chapter23_Crowd_Pathfinding_and_Steering_Using_Flow_Field_Tiles.pdf) |

Keep per-unit simulation state small and cache-friendly. Process units in tight
loops over dense arrays. The win for movement specifically is that **flow fields
amortize pathfinding across an entire army**: compute one vector field per
destination, and every unit just reads its local cell — O(units) movement instead
of O(units × A\*).

---

## 9. Animation

- **Format:** glTF is the standard; the [`gltf`](https://crates.io/crates/gltf)
  crate parses it. Bevy imports glTF (incl. skinned meshes + animation clips)
  out of the box. A from-scratch reference:
  [glTF animations in wgpu and Rust](https://whoisryosuke.com/blog/2022/importing-gltf-with-wgpu-and-rust/).
- **Skeletal animation runtime:** for a deterministic, cross-platform runtime use
  [**ozz-animation-rs**](https://github.com/SlimeYummy/ozz-animation-rs) — a Rust
  port of the well-known [ozz-animation](https://guillaumeblanc.github.io/ozz-animation/)
  that is explicitly "cross-platform deterministic … can be used in network game
  scenarios, such as lock-step networking." That determinism note matters if any
  animation event feeds back into gameplay (it usually shouldn't — keep animation
  in the *presentation* layer).
- **GPU skinning** for crowds: skin in a vertex/compute shader so thousands of
  animated units stay cheap; combine with the instancing path in [§7.3](#73-gpu-driven-rendering-for-crowds).
- **Animation graphs / blending:** Bevy has a built-in animation graph;
  [`skeletal_animation`](https://crates.io/crates/skeletal_animation) offers
  data-driven state machines/blend trees if going custom.

> Keep animation **off** the deterministic sim's critical path. The sim decides
> "unit is attacking"; the renderer decides which frames play. This avoids dragging
> animation timing into desync territory.

---

## 10. Particles & VFX

Use a **GPU compute-driven** particle system so explosions, weapon trails, and
engine exhaust scale to millions of particles with minimal CPU cost:

- [**bevy_hanabi**](https://github.com/djeedai/bevy_hanabi) — a GPU particle system
  for Bevy, "millions of particles simulated in real time" via compute shaders with
  ping-pong buffers and runtime-generated simulation shaders
  ([lib.rs](https://lib.rs/crates/bevy_hanabi),
  [deep-dive series](https://medium.com/@Sou1gh0st/gpu-particle-research-bevy-hanabi-part-1-9797d3f08535)).
- Going custom on wgpu: implement spawn/update/render compute passes yourself; the
  hanabi architecture is a good blueprint.

VFX is purely presentational — never let particle counts or RNG touch the sim.

---

## 11. Lighting

You want "lots of vertex lighting" — i.e. many dynamic lights (weapons fire,
explosions, engine glows, building lights). The technique is **clustered forward
(Forward+) shading**: divide the view frustum into a 3D grid of clusters, assign
each light to the clusters it touches (via a compute pass), and each fragment/vertex
only shades against the handful of lights in its cluster — scaling to thousands of
dynamic lights.

- Bevy already implements clustered forward shading.
- WebGPU/wgpu references:
  [toji's WebGPU clustered shading](https://github.com/toji/webgpu-clustered-shading),
  [a clustered-shading tutorial](https://github.com/DaveH355/clustered-shading),
  and WebGPU Forward+/clustered-deferred renderers handling
  [thousands of dynamic lights](https://github.com/brycej217/WebGPU-Forward-Plus-and-Clustered-Deferred-Renderer).
- Use **logarithmic Z-slicing** for the cluster depth bins (more slices near the
  camera) as those references describe.

For an RTS's top-down/angled camera with many small light sources, clustered
forward is the sweet spot; deferred is an option if you push light counts to the
extreme, at the cost of bandwidth and trickier transparency/MSAA on the web.

---

## 12. Large maps, planets & seamless space transitions

This cluster of features (large maps → seamless space → procedural planets) is the
most technically ambitious. Tackle it as three layers.

### 12.1 Floating origin (precision across vast distances)

`f32` precision degrades with distance from the origin, causing jitter and cracks at
planetary/solar scale. The fix is a **floating origin**: keep the camera near the
origin and move the world, storing absolute positions in higher precision.

- [**big_space**](https://github.com/aevyrie/big_space) — a Bevy floating-origin
  plugin using nested integer grids (i8…i128) for "enough precision to render
  proton-sized meshes across the observable universe," with spatial hashing and no
  added dependencies. This is the production-grade option.
- Background: [Frozen Fractal — Floating the origin](https://frozenfractal.com/blog/2024/4/11/around-the-world-14-floating-the-origin/),
  [Using a Floating Origin for large virtual worlds (paper)](https://www.researchgate.net/publication/331628217_Using_a_Floating_Origin_to_Improve_Fidelity_and_Performance_of_Large_Distributed_Virtual_Worlds).

> Determinism note: a floating origin shifts the *render* frame. Keep the
> **simulation** in its own absolute fixed-point coordinate space so origin shifts
> never perturb gameplay math.

### 12.2 Procedural planets (cube-sphere + quadtree LOD)

The standard approach is a **spherified cube ("quadsphere")**: six quadtree-subdivided
cube faces projected onto a sphere, so each face can reuse ordinary terrain
algorithms, with continuous LOD as the camera approaches.

- LOD scheme: **CDLOD** (Continuous Distance-Dependent LOD) or quadtree
  geomipmapping, with parent-normal-map morphing to avoid popping.
- Surface detail: fractal noise (FBM over Perlin/Simplex) via the
  [`noise`](https://crates.io/crates/noise) crate, ideally evaluated on the GPU.
- References:
  [Leah Lindner — Planet Renderer / LOD terrain research](https://leah-lindner.com/blog/2016/10/10/planetrenderer_week1/),
  [Procedural Planetary Multi-resolution Terrain Generation (arXiv)](https://arxiv.org/pdf/1803.04612),
  [Comparative Analysis of Procedural Planet Generators (arXiv, 2025)](https://arxiv.org/pdf/2510.24764),
  [OpenGL procedural planets: quadtrees & geomipmapping](https://gamedev.net/forums/topic/637956-opengl-procedural-planet-generation-quadtrees-and-geomipmapping/5026604/).

### 12.3 Seamless space ↔ surface transitions

"Seamless" is a *presentation* illusion built from: floating origin (§12.1) + a
**scale hierarchy** (galaxy → system → planet → surface, each its own coordinate
frame that you cross-fade/hand off between) + aggressive LOD streaming so geometry
detail appears as you descend. The gameplay simulation, by contrast, is **not** one
seamless space — model distinct simulation *domains* (a planet-surface battlefield;
a 3D space volume) and hand units between them at defined transition points. Trying
to make one literal continuous simulation from orbit to ground is where projects
drown; fake the seam visually, switch domains logically.

---

## 13. Movement & pathfinding

This is the deepest algorithmic area, and your requirements span **three distinct
navigation domains**. Use a `Navigator` trait per domain behind a common
"give me steering toward goal" interface, so combat code is domain-agnostic.

### 13.1 Surface combat: hierarchical pathfinding + flow fields + ORCA

The proven RTS stack for thousands of units on a battlefield is a **three-tier**
system:

1. **Long-range planning — hierarchical pathfinding.** Partition the map into
   clusters and precompute inter-cluster connectivity so long paths are cheap.
   [**HPA\***](https://github.com/hugoscurti/hierarchical-pathfinding)
   ([Botea & Müller paper](https://www.researchgate.net/publication/228785110_Near_optimal_hierarchical_path-finding_HPA)),
   optionally with [Jump Point Search](https://lucho1.github.io/JumpPointSearch/) on
   uniform grids. Base A\*/Dijkstra primitives:
   [`pathfinding`](https://crates.io/crates/pathfinding).
2. **Group movement — flow fields.** For moving an army to a destination, compute a
   single Dijkstra-derived **vector field**; every unit reads its local cell. This
   is *the* technique for "100s–1000s to a common goal," used in Supreme Commander
   and Planetary Annihilation.
   [How To RTS — flow fields](https://howtorts.github.io/2014/01/04/basic-flow-fields.html),
   [jdxdev flowfields](https://www.jdxdev.com/blog/2020/05/03/flowfields/),
   [Game AI Pro ch.23 (Emerson)](https://www.gameaipro.com/GameAIPro/GameAIPro_Chapter23_Crowd_Pathfinding_and_Steering_Using_Flow_Field_Tiles.pdf).
3. **Local collision avoidance — ORCA.** Units avoid each other and dynamic
   obstacles with **Optimal Reciprocal Collision Avoidance (ORCA/RVO2)**.
   Rust ports: [`dodgy`](https://docs.rs/dodgy) / `dodgy_2d` /
   [`dodgy_3d`](https://docs.rs/dodgy_3d) (ports of
   [RVO2](https://gamma.cs.unc.edu/RVO2/) / [ORCA](https://gamma.cs.unc.edu/ORCA/)).
   For cheaper "feel" you can blend in boids-style separation/alignment/cohesion
   ([Reynolds](https://www.red3d.com/cwr/boids/),
   [Boids for RTS](https://www.jdxdev.com/blog/2021/03/19/boids-for-rts/)).

Where the world is mesh-y rather than grid-y, use a **navmesh**. Rust options:
[`recast_navigation`](https://crates.io/crates/recast_navigation) /
[`divert`](https://lib.rs/crates/divert) (bindings to the industry-standard
[Recast & Detour](https://github.com/recastnavigation/recastnavigation)), or the
pure-Rust port [`rerecast`](https://github.com/janhohenheim/rerecast); for
any-angle navmesh paths the Polyanya algorithm (available via the Bevy navmesh
ecosystem — see [Are We Game Yet? · Pathfinding](https://arewegameyet.rs/ecosystem/)).

> **Determinism:** all of the above must run in the fixed-point sim core. ORCA in
> particular involves linear programming with divisions — port it onto fixed-point
> or it will desync. This is real work; budget for it.

### 13.2 Spherical (planet-surface) pathfinding

On a planet you cannot use one flat grid. Practical approaches, easiest→hardest:

- **Cube-sphere face grids (recommended).** Because the planet is already a
  quadsphere ([§12.2](#122-procedural-planets-cube-sphere--quadtree-lod)), run the
  per-tier surface stack (HPA\* + flow fields + ORCA) **per cube face in that face's
  local 2D parameter space**, stitching paths across the 12 face edges. This reuses
  the entire §13.1 stack with edge bookkeeping.
- **Geodesic / navmesh on the sphere.** Build a navmesh directly on the spherical
  surface (the A\* Pathfinding Project documents
  [spherical worlds](https://arongranberg.com/astar/documentation/spherical_4_1_20_17f940b2/old/spherical.php)
  this way). Great-circle distances are the natural heuristic.
- **Any-angle on a sphere (research-grade).**
  [Optimal Any-Angle Pathfinding on a Sphere (arXiv)](https://arxiv.org/pdf/2004.12781)
  adapts Anya to spherical geometry if you need truly optimal geodesic paths.

> For a shippable game, do the **cube-face-grid** approach. Spherical any-angle is a
> rabbit hole; only go there if surface navigation quality becomes a headline
> feature.

### 13.3 3D space pathfinding

For ships maneuvering in 3D (debris fields, asteroid belts), grids don't fit; use a
**sparse voxel octree (SVO) + 3D A\***:

- Voxelize space into an octree; A\* through it; this is exactly how
  [Mercuna 3D Navigation](https://mercuna.com/3d-navigation/features/) and
  [flying-AI systems](https://blendersleuth.github.io/FlyingNavSystemSite/start.html)
  work. Implementation walkthrough:
  [Unity SVO 3D pathfinding](https://github.com/Gornhoth/Unity-Pathfinding-3D);
  algorithmic grounding:
  [Benchmarks for Pathfinding in 3D Voxel Space (AAAI/SoCS)](https://ojs.aaai.org/index.php/SOCS/article/download/18464/18255/21980).
- **3D collision avoidance:** [`dodgy_3d`](https://docs.rs/dodgy_3d) (3D ORCA) for
  ship-to-ship avoidance; represent capital ships as capsules/cuboids so long hulls
  are avoided accurately.

---

## 14. AI bots (first-class, built alongside)

Because the architecture funnels *all* actions through the command API, an AI bot is
just another **command source** that observes simulation state and emits orders —
identical to a human. Build it from day one; it doubles as your automated test
harness (headless self-play surfaces desyncs and balance issues fast).

Layer the AI like a real RTS opponent (mirroring the
[RTS AI Problems & Techniques survey](https://www.researchgate.net/publication/311176051_RTS_AI_Problems_and_Techniques)):

| Layer | Decides | Technique |
| --- | --- | --- |
| **Strategic** | Build order, tech, economy, expansion | Build-order planners / scripted openings → optionally adaptive (Bayesian build-order inference); **utility AI** to weigh competing goals |
| **Operational** | Where to attack/defend, army composition | **Influence maps / potential fields** for threat & territory ([Game AI Pro influence maps]; used for unit navigation and kiting) |
| **Tactical** | Squad maneuvers, focus fire, retreats | **Behavior trees** + utility scoring ([behavior trees explained](https://www.gamedeveloper.com/programming/behavior-trees-for-ai-how-they-work), [utility-based BTs](https://anshuman-kumar.gitbook.io/nez-doc/ai-fsm-behavior-tree-goap-utility-ai)) |
| **Micro** | Individual unit steering/kiting | Reuses the same flow-field + ORCA movement as players ([§13.1](#131-surface-combat-hierarchical-pathfinding--flow-fields--orca)) |

- **Behavior trees** give readable, authorable tactical logic; **utility AI** shines
  for the "many competing options" decisions an RTS constantly faces
  ([parameterized BTs in RTS, arXiv](https://arxiv.org/pdf/2111.12144)). Combine
  them (utility-driven BTs).
- **Influence maps** are the workhorse for spatial reasoning (threat, safety, where
  the front line is) and are cheap to compute over the same grid the flow fields use.
- Rust crates exist for BTs (e.g. [`bonsai-bt`](https://crates.io/crates/bonsai-bt));
  utility/influence layers are simple enough to own. Keep all AI computation either
  inside the deterministic sim (if it must be replay-identical) or clearly outside it
  (if bots may diverge cosmetically) — pick one and be consistent.
- Long-term: the same headless sim core is what you'd plug a learning agent into
  (cf. StarCraft's BWAPI/SC2 research ecosystems), but rules-based AI is the right
  first target.

---

## 15. Web target constraints

Web-first imposes real limits — design for them now, not after they bite:

- **Threading.** Multithreaded WASM needs `SharedArrayBuffer`, which requires
  **cross-origin isolation** (COOP/COEP headers) and Wasm atomics/bulk-memory.
  Without it, your WASM build is single-threaded. Bevy *can* run multithreaded on
  the web with the right build flags, but plan for: (a) a deterministic sim core
  that runs comfortably **single-threaded** at 20–30 Hz, and (b) shipping those
  headers from your host/CDN. The sim/render split helps — the heavy parallelism
  (rendering) is on the GPU, and the sim is light.
- **WebGPU vs WebGL2.** Prefer WebGPU (compute shaders → GPU particles, GPU culling,
  GPU skinning all become possible). Keep a WebGL2 fallback path for older browsers,
  accepting reduced effects there. wgpu abstracts both
  ([web.dev support status](https://web.dev/blog/webgpu-supported-major-browsers)).
- **Binary size & load time.** WASM download size matters; budget for `wasm-opt`,
  release LTO, and asset streaming. egui keeps UI binary cost low.
- **Memory.** A WASM tab has a finite linear-memory budget; large procedural worlds
  must stream, not preload.
- **Determinism parity.** Fixed-point math (§5) makes WASM↔native bit-parity a
  non-issue — a major reason to choose it for a web-first competitive game.

Tooling: [`trunk`](https://crates.io/crates/trunk) and `wasm-bindgen` for web builds.
A [SessionStart hook](https://code.claude.com/docs/en/claude-code-on-the-web) can keep
web CI building both targets.

---

## 16. Proposed Cargo workspace layout

A workspace that enforces the architectural seams (the sim crate must not depend on
Bevy/wgpu — that's the whole point):

```
rts-99-jam/
├── Cargo.toml                  # workspace
├── crates/
│   ├── sim/                    # ⭐ deterministic core: NO engine deps
│   │   ├── fixed-point math (fixed + cordic), seeded PRNG
│   │   ├── ECS/SoA world, components, systems (fixed 20–30 Hz tick)
│   │   ├── commands (the ONLY way to mutate state)
│   │   ├── pathfinding (HPA*, flow fields, ORCA) — fixed-point
│   │   └── state checksum / snapshot / replay
│   ├── ai/                     # command sources; depends on `sim` only
│   │   └── strategic / operational / tactical / micro layers
│   ├── net/                    # transport trait + lockstep coordinator
│   │   ├── matchbox (WebRTC) impl, websocket signaling, quinn (native)
│   │   └── turn scheduling, input delay, desync detection
│   ├── render/                 # Bevy plugins: instancing, LOD, clustered lights
│   ├── planet/                 # cube-sphere LOD, noise, big_space integration
│   ├── assets/                 # glTF load, animation graphs, VFX defs
│   └── app/                    # binary: wires Bevy + sim + net + ai + ui
├── server/                     # headless: relay + matchmaking + optional auth sim
│   └── reuses `sim` + `net`
└── docs/
    └── architecture.md         # this file
```

Key invariant enforced by the layout: **`sim` and `ai` compile with zero rendering
dependencies**, so the dedicated server and CI self-play harness are just thin
binaries over the same code that runs in the browser.

---

## 17. Consolidated technology stack

| Concern | Primary pick | Alternatives / notes |
| --- | --- | --- |
| Engine substrate | [Bevy](https://bevy.org/) (on wgpu) | Custom [wgpu](https://wgpu.rs/) + [winit](https://crates.io/crates/winit) |
| Graphics API | [wgpu](https://github.com/gfx-rs/wgpu) (WebGPU + WebGL2) | — |
| UI | [egui](https://github.com/emilk/egui) (tools/HUD/overlays) | [iced](https://iced.rs), `bevy_ui` |
| Sim ECS | `bevy_ecs` (standalone, fixed order) or [`hecs`](https://crates.io/crates/hecs) | hand-rolled SoA |
| Determinism math | [`fixed`](https://crates.io/crates/fixed) + [`cordic`](https://crates.io/crates/cordic) | controlled `f32` ([Rapier determinism](https://rapier.rs/docs/user_guides/rust/determinism/)) |
| Render math | [`glam`](https://crates.io/crates/glam) (SIMD) | — (never in sim core) |
| Netcode transport | [Matchbox](https://github.com/johanhelsing/matchbox) (WebRTC) | WebTransport (`wtransport`), [`quinn`](https://crates.io/crates/quinn) native, WebSocket signaling |
| Netcode model | custom lockstep on Matchbox | [Lightyear](https://github.com/cBournhonesque/lightyear), [GGRS](https://github.com/gschup/ggrs) (rollback), [renet](https://github.com/lucaspoffo/renet) |
| Particles | [bevy_hanabi](https://github.com/djeedai/bevy_hanabi) | custom wgpu compute |
| Lighting | clustered forward (Bevy built-in) | [toji clustered shading](https://github.com/toji/webgpu-clustered-shading) |
| Animation | [ozz-animation-rs](https://github.com/SlimeYummy/ozz-animation-rs) + [`gltf`](https://crates.io/crates/gltf) | Bevy animation graph, [`skeletal_animation`](https://crates.io/crates/skeletal_animation) |
| Crowd rendering | GPU instancing + indirect + impostors | [renderling](https://github.com/schell/renderling) |
| Large worlds | [big_space](https://github.com/aevyrie/big_space) (floating origin) | — |
| Planets | cube-sphere + CDLOD + [`noise`](https://crates.io/crates/noise) | [planet renderer refs](https://leah-lindner.com/blog/2016/10/10/planetrenderer_week1/) |
| Grid pathfinding | [`pathfinding`](https://crates.io/crates/pathfinding) + [HPA\*](https://github.com/hugoscurti/hierarchical-pathfinding) + flow fields | [JPS](https://lucho1.github.io/JumpPointSearch/) |
| Navmesh | [`recast_navigation`](https://crates.io/crates/recast_navigation) / [`rerecast`](https://github.com/janhohenheim/rerecast) | [`divert`](https://lib.rs/crates/divert) |
| Collision avoidance | [`dodgy`](https://docs.rs/dodgy) / [`dodgy_3d`](https://docs.rs/dodgy_3d) (ORCA) | boids steering |
| 3D space nav | sparse-voxel-octree A\* | [Mercuna-style](https://mercuna.com/3d-navigation/features/) |
| AI | behavior trees ([`bonsai-bt`](https://crates.io/crates/bonsai-bt)) + utility + influence maps | — |
| Audio | [`kira`](https://crates.io/crates/kira) | `bevy_audio` |
| Web build | [`trunk`](https://crates.io/crates/trunk) + `wasm-bindgen` | — |
| Ecosystem index | [Are We Game Yet?](https://arewegameyet.rs/) | [awesome-bevy](https://github.com/nolantait/awesome-bevy) |

---

## 18. Phased roadmap

Sequenced so there is something *playable* at every stage and the hard research is
deferred until the foundation is proven. Each phase ends in a demoable build.

### Phase 0 — Foundation & determinism spike (de-risk the core)
- Cargo workspace; `sim` crate with fixed-point math, seeded PRNG, fixed 30 Hz tick.
- Command system + one unit type that can move. Bevy app renders interpolated state.
- **Determinism test:** run the same command log on native + WASM; assert identical
  end-state checksums. *If this doesn't pass, nothing downstream works.*

### Phase 1 — Single-player RTS slice (flat map)
- Grid map, HPA\* + flow fields + ORCA movement, a few unit types, resources,
  one building, combat. egui debug HUD. GPU-instanced rendering. AI bot v0
  (scripted build order + flow-field attack) — built here, not later.

### Phase 2 — Multiplayer lockstep (flat map)
- Matchbox WebRTC transport + WebSocket signaling/lobby; star-topology input relay;
  lockstep turn scheduler with input delay; desync detection via checksums; replays.
- Ship a 1v1 (and vs-AI) browser build behind COOP/COEP. This is the real MVP.

### Phase 3 — Scale & polish
- Push to 1000s of units (GPU culling/LOD, impostors); clustered lighting;
  bevy_hanabi VFX; skeletal animation; audio; bigger maps with streaming;
  AI up to operational/tactical layers with influence maps.

### Phase 4 — Planets & space (the ambitious layer)
- big_space floating origin; cube-sphere procedural planets with CDLOD;
  per-cube-face surface pathfinding; SVO 3D space navigation + 3D ORCA;
  visually seamless space↔surface transitions with domain hand-off.

> If schedule pressure hits, **Phases 0–2 are the game**; Phases 3–4 are what make
> it special. Cutting Phase 4 leaves a complete StarCraft-style RTS.

---

## 19. Risk register & open questions

| Risk | Severity | Mitigation |
| --- | --- | --- |
| Cross-platform desyncs (native↔WASM) | **Critical** | Fixed-point sim core; checksum tests in CI from Phase 0; no `HashMap` iteration, no SIMD in sim |
| Porting ORCA/pathfinding to fixed-point | High | Budget explicit time; keep a float reference impl for diffing |
| WASM multithreading / COOP-COEP hosting | High | Single-threaded sim by design; verify header setup early |
| Seamless space↔surface over-scoping | High | Fake the seam visually; switch sim *domains* logically; defer to Phase 4 |
| Spherical pathfinding complexity | Medium | Use cube-face grids, not geodesic any-angle, unless it's a headline feature |
| Lockstep maphack/anti-cheat | Medium | Decide consciously (accept, or add authoritative fog sim) |
| Bevy upgrade churn | Medium | Isolate Bevy behind `render`/`app`; keep `sim`/`ai`/`net` engine-free |
| WASM binary size / load time | Medium | `wasm-opt`, LTO, asset streaming, egui for UI |

**Open questions to resolve before/while building:**
1. **Is `rts-99-jam` a time-boxed game jam?** If so, scope to Phase 1 (single-player
   flat-map slice) — Phases 2–4 are post-jam. This materially changes everything.
2. **Lockstep input-delay vs. rollback** — is hidden-latency input delay acceptable
   (yes for most RTS), or do you want rollback responsiveness despite the
   re-sim cost at high unit counts?
3. **Anti-cheat posture** — accept maphack risk (standard for lockstep) or invest in
   an authoritative server sim?
4. **Art pipeline** — stylized low-poly (cheaper to animate/instance at scale, better
   for web) vs. detailed? This drives the rendering budget hard.
5. **Bevy vs. custom wgpu** — confirm the Option C hybrid, or do you specifically
   want to avoid Bevy and build directly on wgpu (much more plumbing)?

---

## References

Grouped by topic; all links verified against current sources during research.

**Engine / rendering foundation**
- Bevy — https://bevy.org/ · https://github.com/bevyengine/bevy
- wgpu — https://wgpu.rs/ · https://github.com/gfx-rs/wgpu · Learn wgpu — https://sotrh.github.io/learn-wgpu/
- WebGPU browser support — https://web.dev/blog/webgpu-supported-major-browsers
- Browser games with WASM/WebGPU/Rust — https://techbytes.app/posts/build-browser-games-wasm-webgpu-rust-2026/
- renderling (GPU-driven wgpu renderer) — https://github.com/schell/renderling
- Are We Game Yet? (ecosystem index) — https://arewegameyet.rs/ · awesome-bevy — https://github.com/nolantait/awesome-bevy

**UI**
- egui — https://github.com/emilk/egui · iced — https://iced.rs
- Rust GUI comparison (2025) — https://an4t.com/rust-gui-libraries-compared/
- egui vs iced for game integration — https://users.rust-lang.org/t/egui-vs-iced-in-regards-to-game-engine-integration/74569

**Determinism & simulation**
- Deterministic Lockstep (Fiedler) — https://gafferongames.com/post/deterministic_lockstep/
- Floating Point Determinism (Fiedler) — https://gafferongames.com/post/floating_point_determinism/
- Cross-platform RTS sync & FP indeterminism — https://www.gamedeveloper.com/programming/cross-platform-rts-synchronization-and-floating-point-indeterminism
- Rapier determinism — https://rapier.rs/docs/user_guides/rust/determinism/
- Bevy "Auditing determinism" — https://github.com/bevyengine/bevy/discussions/2480
- `fixed` — https://crates.io/crates/fixed · `cordic` — https://crates.io/crates/cordic
- RTS-in-ECS design — https://github.com/bevyengine/bevy/discussions/2659

**Netcode**
- Matchbox (WebRTC P2P) — https://github.com/johanhelsing/matchbox · intro — https://johanhelsing.studio/posts/introducing-matchbox · 0.6 — https://johanhelsing.studio/posts/matchbox-0-6
- Extreme Bevy (p2p web rollback tutorial) — https://johanhelsing.studio/posts/extreme-bevy
- Rock and Rollback (Bevy browser multiplayer) — https://www.metabrew.com/article/rock-and-rollback-realtime-multiplayer-games-with-bevy
- GGRS (rollback) — https://github.com/gschup/ggrs
- Lightyear — https://github.com/cBournhonesque/lightyear · https://docs.rs/lightyear/latest/lightyear/
- renet — https://github.com/lucaspoffo/renet · bevy_quinnet — https://github.com/Henauxg/bevy_quinnet · quinn — https://crates.io/crates/quinn
- netcode protocol — https://github.com/benny-n/netcode
- WebTransport vs WebRTC vs WebSockets — https://webrtchacks.com/webcodecs-webtransport-and-webrtc/ · https://developers.rune.ai/blog/webrtc-vs-websockets-for-multiplayer-games · HTTP/3 + UDP tunnels — https://dev.to/instatunnel/beyond-http-exposing-webrtc-and-local-game-servers-via-udp-tunnels-5ak5

**Rendering at scale / lighting / particles / animation**
- GPU-driven rendering — https://vkguide.dev/docs/gpudriven/gpu_driven_engines/ · GPU-driven instancing — https://playerunknownproductions.net/news/gpu-driven-instancing
- Impostors & pseudo-instancing for crowds — https://www.researchgate.net/publication/220979001_Impostors_and_pseudo-instancing_for_GPU_crowd_rendering
- Clustered shading — https://github.com/toji/webgpu-clustered-shading · https://github.com/DaveH355/clustered-shading · https://github.com/brycej217/WebGPU-Forward-Plus-and-Clustered-Deferred-Renderer
- bevy_hanabi (GPU particles) — https://github.com/djeedai/bevy_hanabi · https://lib.rs/crates/bevy_hanabi
- ozz-animation-rs — https://github.com/SlimeYummy/ozz-animation-rs · ozz-animation — https://guillaumeblanc.github.io/ozz-animation/ · gltf crate — https://crates.io/crates/gltf · glTF+wgpu tutorial — https://whoisryosuke.com/blog/2022/importing-gltf-with-wgpu-and-rust/

**Large worlds / planets**
- big_space (floating origin) — https://github.com/aevyrie/big_space
- Floating origin background — https://frozenfractal.com/blog/2024/4/11/around-the-world-14-floating-the-origin/ · https://www.researchgate.net/publication/331628217_Using_a_Floating_Origin_to_Improve_Fidelity_and_Performance_of_Large_Distributed_Virtual_Worlds
- Procedural planets — https://leah-lindner.com/blog/2016/10/10/planetrenderer_week1/ · https://arxiv.org/pdf/1803.04612 · https://arxiv.org/pdf/2510.24764 · https://gamedev.net/forums/topic/637956-opengl-procedural-planet-generation-quadtrees-and-geomipmapping/5026604/
- noise crate — https://crates.io/crates/noise

**Pathfinding & movement**
- Flow fields — https://howtorts.github.io/2014/01/04/basic-flow-fields.html · https://www.jdxdev.com/blog/2020/05/03/flowfields/ · Game AI Pro ch.23 — https://www.gameaipro.com/GameAIPro/GameAIPro_Chapter23_Crowd_Pathfinding_and_Steering_Using_Flow_Field_Tiles.pdf
- HPA\* — https://github.com/hugoscurti/hierarchical-pathfinding · https://www.researchgate.net/publication/228785110_Near_optimal_hierarchical_path-finding_HPA · JPS — https://lucho1.github.io/JumpPointSearch/ · pathfinding crate — https://crates.io/crates/pathfinding
- Recast/Detour — https://github.com/recastnavigation/recastnavigation · recast_navigation — https://crates.io/crates/recast_navigation · rerecast — https://github.com/janhohenheim/rerecast · divert — https://lib.rs/crates/divert
- ORCA/RVO2 — https://gamma.cs.unc.edu/ORCA/ · https://gamma.cs.unc.edu/RVO2/ · dodgy — https://docs.rs/dodgy · dodgy_3d — https://docs.rs/dodgy_3d
- Boids — https://www.red3d.com/cwr/boids/ · Boids for RTS — https://www.jdxdev.com/blog/2021/03/19/boids-for-rts/
- Spherical pathfinding — https://arongranberg.com/astar/documentation/spherical_4_1_20_17f940b2/old/spherical.php · Optimal Any-Angle on a Sphere — https://arxiv.org/pdf/2004.12781
- 3D / space pathfinding — https://mercuna.com/3d-navigation/features/ · https://blendersleuth.github.io/FlyingNavSystemSite/start.html · https://github.com/Gornhoth/Unity-Pathfinding-3D · https://ojs.aaai.org/index.php/SOCS/article/download/18464/18255/21980

**AI**
- RTS AI Problems & Techniques (survey) — https://www.researchgate.net/publication/311176051_RTS_AI_Problems_and_Techniques
- Behavior trees — https://www.gamedeveloper.com/programming/behavior-trees-for-ai-how-they-work · FSM/BT/GOAP/Utility overview — https://anshuman-kumar.gitbook.io/nez-doc/ai-fsm-behavior-tree-goap-utility-ai
- Parameterized BTs in RTS — https://arxiv.org/pdf/2111.12144
- bonsai-bt — https://crates.io/crates/bonsai-bt

**Web tooling**
- trunk — https://crates.io/crates/trunk · Claude Code on the web — https://code.claude.com/docs/en/claude-code-on-the-web
