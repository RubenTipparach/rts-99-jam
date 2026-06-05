# rts-99-jam

A **web-first, StarCraft-style multiplayer RTS** built in Rust.

- **Stack:** Rust + [wgpu](https://wgpu.rs/) (via [Bevy](https://bevy.org/)) +
  [egui](https://github.com/emilk/egui) for tooling/UI.
- **Targets:** browser (WASM + WebGPU, with WebGL2 fallback) first, native desktop second.
- **Multiplayer:** server-assisted **deterministic lockstep** (P2P inputs over WebRTC,
  relayed/coordinated by a light session server).
- **Ambition:** 100s–1000s of units at 120–160 fps, GPU-driven rendering, procedural
  planets, and seamless space↔surface transitions — sequenced so a playable game ships
  long before the showpiece features.

## Architecture

The defining idea is a **deterministic simulation core** (engine-agnostic, fixed-point,
fixed-tick) that is fully decoupled from the **Bevy/wgpu presentation layer**. Humans,
AI bots, network peers, and replays are all just *command sources* feeding that core.
This single seam is what makes lockstep netcode, replays, server validation, and
headless AI fall out almost for free.

📄 **Design & architecture: [`docs/architecture.md`](docs/architecture.md)**

Surveys the technologies and algorithms for every engine requirement — rendering,
animation, particles, lighting, netcode, large maps, procedural planets, surface /
spherical / 3D-space pathfinding, collision avoidance, and AI — each with primary
references, and proposes a concrete workspace layout, tech stack, phased roadmap, and
risk register.

🪐 **Ground-to-space: [`docs/ground-to-space.md`](docs/ground-to-space.md)**

The planetary/orbital/space layer — nested reference frames (focusing a planet = a
change of basis), sim-LOD vs. render-LOD, the orbital ephemeris, the ship flight model
with the **Lambert ↔ brachistochrone trajectory spectrum**, Clohessy–Wiltshire orbital
combat, and the construction → mobility → combat loop where one physics model generates
the strategy.

## Status

Design phase. See the [phased roadmap](docs/architecture.md#18-phased-roadmap) — the MVP
(Phases 0–2) is single-player then lockstep multiplayer on a flat map; planets and space
(Phase 4) are the ambitious final layer.
