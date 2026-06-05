<p align="center">
  <img src="assets/branding/logo.png" alt="Sol Dominion (working title)" width="680">
</p>

<p align="center"><em>Working title — branding in <a href="assets/branding/">assets/branding/</a></em></p>

# rts-99-jam

A deterministic, lockstep real-time strategy engine in the lineage of *StarCraft*
and *Planetary Annihilation* — built in **Rust** with a **wgpu** renderer (no
Bevy).

Targets: 100s–1000s of units, **120–160 FPS**, large flat **or** spherical maps,
procedural generation, peer-to-peer multiplayer with server assist, and
first-class AI bots.

## Architecture

The design is documented in full:

- **[ARCHITECTURE.md](ARCHITECTURE.md)** — start here: the two invariants, the
  dual-clock game loop, the crate workspace, and the chapter index.
- **[docs/architecture/](docs/architecture/)** — one chapter per concern:

  | # | Chapter | Topic |
  |---|---------|-------|
  | 00 | [Overview](docs/architecture/00-overview.md) | Vision, glossary, prior art |
  | 01 | [Determinism](docs/architecture/01-determinism.md) | Fixed-point, RNG, ordering, desync detection |
  | 02 | [Simulation](docs/architecture/02-simulation.md) | SoA data model, 1000s of units |
  | 03 | [Networking & Lockstep](docs/architecture/03-networking-lockstep.md) | WebRTC, server-assist, replays |
  | 04 | [Rendering (wgpu)](docs/architecture/04-rendering-wgpu.md) | Instancing, vertex/clustered lighting, scale |
  | 05 | [Animation](docs/architecture/05-animation.md) | Baked bone textures at crowd scale |
  | 06 | [Particles](docs/architecture/06-particles.md) | GPU-driven, cosmetic |
  | 07 | [Pathfinding](docs/architecture/07-pathfinding-navigation.md) | Flow fields, HPA*, ORCA, spherical |
  | 08 | [Procedural Generation](docs/architecture/08-procedural-generation.md) | Deterministic large maps |
  | 09 | [AI Bots](docs/architecture/09-ai-bots.md) | "AI is a player," built alongside |
  | 10 | [Roadmap & Testing](docs/architecture/10-roadmap-testing.md) | Milestones, determinism CI, tooling |

## The one-paragraph summary

Only **commands** cross the network; every machine runs a **bit-identical
deterministic simulation** (fixed-point math, no floats) so 1000s of units cost
almost no bandwidth. The sim ticks slowly and exactly (~25 Hz) while the **wgpu**
renderer **interpolates** between sim states at 120–160 FPS. Scale comes from GPU
instancing + indirect culling (rendering) and flow-field pathing + spatial hashing
(simulation). **AI bots emit the same commands a human does**, run deterministically
in-sim at no network cost, and double as the automated test harness. See
[ARCHITECTURE.md](ARCHITECTURE.md) for how these fit together.

## Status

Design phase. Implementation follows the
[roadmap](docs/architecture/10-roadmap-testing.md), starting with **Milestone 0:
prove determinism before writing a single shader.**
