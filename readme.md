<p align="center">
  <img src="assets/branding/logo.png" alt="Astromancers - Space · Magic · Real-Time Strategy" width="760">
</p>

<p align="center"><em>Brand kit in <a href="assets/branding/">assets/branding/</a></em></p>

# Astromancers

A deterministic, lockstep real-time strategy game of **space and magic**, in the
lineage of *StarCraft* and *Planetary Annihilation* - built in **Rust** with a
**wgpu** renderer (no Bevy). *(Repo: `rts-99-jam`.)*

Targets: 100s–1000s of units, **120–160 FPS**, large flat **or** spherical maps,
procedural generation, peer-to-peer multiplayer with server assist, and
first-class AI bots.

## Architecture

The design is documented in full:

- **[ARCHITECTURE.md](docs/ARCHITECTURE.md)** - start here: the two invariants, the
  dual-clock game loop, the crate workspace, and the chapter index.
- **[docs/architecture/](docs/architecture/)** - one chapter per concern:

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
[ARCHITECTURE.md](docs/ARCHITECTURE.md) for how these fit together.

## Status

**Milestone 0 - deterministic core (in progress).** The foundation is built and
tested: a Cargo workspace with dependency-free, integer-only crates -
`math` (fixed-point scalar/vector), `protocol` (commands), `sim` (generational-arena
SoA world + pinned RNG + FNV state hash + `step()`), `replay`, and a `testkit`
headless harness. The keystone determinism test pins a cross-platform state hash,
and CI enforces fmt, clippy (`-D warnings`), the no-floats guard, and a `wasm32`
build. Next milestones follow the
[roadmap](docs/architecture/10-roadmap-testing.md).

## Building

```bash
cargo test --workspace                      # unit + determinism tests
cargo run -p testkit --bin demo_hash        # prints the pinned state hash
```

The simulation crates (`math`, `sim`, `protocol`, `replay`) are `#![forbid(unsafe_code)]`,
float-free, and build for native and `wasm32-unknown-unknown`.
