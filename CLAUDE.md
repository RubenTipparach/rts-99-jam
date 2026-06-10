# Astromancers - project guide

A deterministic, lockstep RTS in Rust + wgpu (not Bevy), built to run native and
on the web (WASM, GitHub Pages). Design: **two worlds, one wall** - a fixed-point
integer simulation produces all game truth; the float-based client only presents
it. "Commands in, snapshots out." See `docs/ARCHITECTURE.md`.

## House style

- **No em-dashes (Unicode U+2014) anywhere in this repo.** Not in docs, code,
  comments, strings, commit messages, or assets. Use a comma, a colon,
  parentheses, or a spaced hyphen (` - `) instead, always with a plain ASCII
  hyphen `-`.
- **Numeric ranges use a plain hyphen,** e.g. `120-160 FPS`, `100s-1000s`,
  `2005-2050`: never an en-dash (Unicode U+2013).

## Workspace layout

- `crates/math` - `Fx` fixed-point scalar (`I40F24`) + `Vec2`/`Vec3`. Integer-only.
- `crates/protocol` - wire `Command`s (the only thing the network carries).
- `crates/sim` - the deterministic `World`: entities, production, movement,
  combat, fog-free game truth. **All game logic that affects state lives here.**
- `crates/replay` - recorded command logs.
- `crates/testkit` - headless harness + the pinned determinism test.
- `apps/client` - wgpu/winit renderer, camera, HUD, fog-of-war, input → commands.
- `docs/` - all project documentation. **Every `.md` file lives in `docs/`,**
  except the root `CLAUDE.md` and `README` files (the root `readme.md`, plus
  conventional folder-level `README.md`s such as `assets/branding/README.md`).

## Determinism rules (do not break these)

- **No floats and no nondeterminism in the sim crates.** CI greps
  `crates/{math,sim,protocol,replay}/src` and fails on `f32`, `f64`, `HashMap`,
  `HashSet`, `Instant`, `SystemTime`, or `thread_rng` - **including in comments**.
  Use `Fx` and ordered `Vec`/index iteration. Floats are allowed only in
  `apps/client` (presentation) and at the sim→render boundary.
- **Iterate in a fixed order** (ascending index). Never let iteration order or
  hashing affect state.
- **The golden hash is pinned** in `crates/testkit/tests/determinism.rs`. Any
  intentional change to sim/math/hash behavior changes it. Regenerate with
  `cargo run -p testkit --bin demo_hash` and paste the new value into `GOLDEN`.
- The sim RNG is the in-state pinned `DetRng`; advancing it changes the hash, so
  only draw from it when the gameplay genuinely needs randomness.

## Gameplay / sim conventions

- **Units never stack on the same spot.** Infantry have collision avoidance:
  each tick a unit is pushed away from any other infantry closer than `SEP_DIST`,
  so a crowd drifts apart and a group settles into distinct cells. This lives in
  `World::separate` (`crates/sim/src/lib.rs`) - fixed-point, computed from one
  consistent snapshot (read all, then apply) so it stays order-independent. Push
  magnitude is proportional to overlap and clamped to `SEP_MAX`, so it's a smooth
  drift that settles exactly at `SEP_DIST` with no oscillation.
- **Group-move orders spread into a formation.** When several units are ordered
  to a point, the client (`Game::order`) assigns each a distinct grid cell around
  the click rather than one shared target; the sim's separation then keeps them
  apart on arrival. Never issue the same destination to every selected unit.
- If you add new mobile unit kinds, give them separation too (and a sensible
  per-kind spacing) so the no-stacking rule holds.
- **No auto-production and no passive income.** Every unit is queued by
  `Command::Train`, and every resource is mined by a worker carrying loads
  from a node to a drop-off - buildings never generate income on their own.
  See `STARTING_ORE`/`TRAIN_COST` in `crates/sim`. The player drives
  production from the HUD command card; the enemy is static until an AI
  issues `Train`. Ore is part of the state hash, so tuning it re-pins the
  golden value.

## Game feel / fx

- **Every interaction gets visual feedback.** Whenever a unit interacts with
  another object - mining, shooting, taking damage, dying, building - there
  must be a visible response: an animation, particles, or a light, usually
  via the client fx layer (`apps/client/src/fx.rs`) driven by sim snapshots
  and the sim's per-tick `shots()` events. Never add an interaction that
  happens silently.
- **Lean on point lights.** The renderer is vertex-lit and supports many
  dynamic point lights cheaply (`gfx::MAX_LIGHTS` slots fed each frame), so
  fx should use them freely - muzzle flashes, mining glints, explosions all
  carry a light, not just particles.

## HUD text policy

- **No tutorial, help, or debug text in the game HUD.** Never render control
  explanations ("left: select", "Esc: pause"), force counters, faction
  labels, or debug readouts on screen; debug info belongs in the console.
  Hotkey hints may appear only as short labels on the command-card buttons
  themselves ("Worker [T]"). UI affordances are communicated by the elements
  themselves (hover/press states, glows, pings), not by explanatory text.

## Branding

- **Never say "deterministic" or "lockstep" in anything user-facing**: UI,
  menus, taglines, store/promo copy, or branding assets. Deterministic
  lockstep is not a selling point, it is the baseline gold standard for any
  multiplayer RTS; keep the term in code and docs only.

## Web UI policy (DOM vs WASM)

This game targets **desktop**. **All functional game UI is rendered from
Rust/WASM** - the wgpu scene plus the Rust-driven HUD canvas (`hud.rs`). Do
**not** add HTML/DOM widgets (buttons, menus, panels, overlays) for gameplay.

The **only** DOM controls allowed are **mobile test controls**, so a developer on
a phone can exercise desktop interactions: a **pan** d-pad, **zoom** in/out, and a
**right-click** toggle - *nothing else*. They live in `apps/client/index.html` as
bare elements; all their behavior is wired from Rust (the `mobile` module in
`main.rs`), and they're shown only on touch devices via `@media (pointer:
coarse)`. The game itself is not meant to be played on mobile.

(The pre-WASM `#loading` overlay is exempt: it must be DOM because it shows
before the WASM module has loaded.)

## Build / test / run

- `cargo test --workspace` - unit tests + the pinned determinism test.
- `cargo clippy --workspace --all-targets -- -D warnings` - must be clean.
- `cargo fmt --all` - must be clean (CI checks `--check`).
- `cargo run -p client` - native window.
- `cd apps/client && trunk serve` - run in the browser; `trunk build --release
  --public-url /rts-99-jam/` is what deploys to GitHub Pages.
- CI toolchain is pinned to Rust **1.94.1**; match it locally to avoid clippy
  version drift.

## Textures

Tile textures are generated by `assets/gen_textures.py` (pure stdlib, no deps).
Terrain tiles and the fog-of-war field are sampled with linear filtering: the
ground gets a soft painterly blur instead of hard texel blocks, and the fog
stays feathered at its borders.
