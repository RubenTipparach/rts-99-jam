# Ground-to-Space Design

> Companion to [`architecture.md`](architecture.md). Specifies the planetary /
> orbital / space layer: reference frames, the orbital simulation, the ship
> flight model (including the **Lambert ↔ brachistochrone trajectory spectrum**),
> space combat, the colonization economy, and how the realistic physics *generates*
> the strategy. Decisions captured here were settled during design review; see
> [§11](#11-decisions-locked--still-open).
>
> Design thesis: **one physical model drives everything.** Construction sets a
> ship's thrust, Δv, power and heat; those set its trajectory options and combat
> reach; combat targets the subsystems that produce them. Realism isn't a skin —
> it's the source of the strategy.

## Table of contents

1. [The two-layer simulation](#1-the-two-layer-simulation)
2. [Reference frames & the "projection"](#2-reference-frames--the-projection)
3. [Sim-LOD vs render-LOD (the lockstep rule)](#3-sim-lod-vs-render-lod-the-lockstep-rule)
4. [Orbital ephemeris](#4-orbital-ephemeris)
5. [Ship flight model](#5-ship-flight-model)
6. [The trajectory spectrum: Lambert ↔ brachistochrone](#6-the-trajectory-spectrum-lambert--brachistochrone)
7. [Close-combat: Clohessy–Wiltshire relative motion](#7-close-combat-clohessywiltshire-relative-motion)
8. [The construction → mobility → combat loop](#8-the-construction--mobility--combat-loop)
9. [Space combat](#9-space-combat)
10. [Ground: colonization, prospecting, launch asymmetry](#10-ground-colonization-prospecting-launch-asymmetry)
11. [Decisions locked & still open](#11-decisions-locked--still-open)
12. [References](#references)

---

## 1. The two-layer simulation

There is **one deterministic source-of-truth simulation**, organized as two coupled layers running at different cadences inside the fixed-point sim core:

| Layer | Frame | Cadence | Contents |
| --- | --- | --- | --- |
| **System layer** | Sun-centered inertial (heliocentric) | low (orbits evolve slowly; can sub-tick) | planet/moon ephemeris, ship orbits & transfers (patched conics), interplanetary freight |
| **Surface/local layer** | per-body planet-fixed + planet-centered inertial | full sim tick (20–30 Hz) | ground RTS units, buildings, economy, **per-planet pathfinding**, local-orbit ship combat |

The layers are coupled only at **hand-off points**: launch/landing (surface ↔ local orbit) and SOI transitions (local orbit ↔ interplanetary). Everything is deterministic and fixed-point so all clients stay bit-identical (see [architecture.md §5](architecture.md#5-the-deterministic-simulation-core)).

---

## 2. Reference frames & the "projection"

A nested reference-frame hierarchy ([game-math primer](https://gamemath.com/book/multiplespaces.html), [NASA NAIF frames](https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/Tutorials/pdf/individual_docs/17_frames_and_coordinate_systems.pdf)):

```
Heliocentric inertial (truth)
└── Planet-centered inertial  (ship orbits; non-rotating)        ── related by R(t) ──┐
    └── Planet-fixed rotating (ground units, buildings, paths)  ←───────────────────┘
        └── (moons nest the same way under their planet)
```

"Focusing" a planet selects the **render origin** and instantiates its two local frames. The orbital truth is *projected* into the focused frame — your "projection problem" is a change of basis:

```
B_local = R(t)⁻¹ · (B_truth − P(t))
```

where `P(t)`, `R(t)` are the planet's position and spin from the ephemeris. Consequences, all free and consistent because there is a single truth sim:

- The **sun arcs across the sky** from `R(t)` (day/night) and drifts seasonally from `P(t)`.
- **Ground units live in the rotating frame** → stationary surface, stable positions, float-safe pathfinding. This is why the planet-as-origin is stable: gameplay math never sees large or fast-changing coordinates.
- **Ships orbit in the planet-centered inertial frame** → orbits don't smear with planetary spin.
- **Launch/landing is the hand-off** across `R(t)` between the two frames.

Pathfinding nav data is generated **deterministically from the planet's noise seed** at a fixed resolution, independent of the rendered LOD geometry (the renderer may show million-poly terrain; the sim navigates a coarse, fixed, seed-derived heightfield). Same seed on every client ⇒ identical nav grid.

---

## 3. Sim-LOD vs render-LOD (the lockstep rule)

> **Focus gates *rendering*, never *simulation*.** In deterministic lockstep every
> client simulates everything from the shared input stream; if one client's camera
> changed what it simulated, clients would desync.

Two independent LOD axes:

| | Driven by | Determinism | Behavior |
| --- | --- | --- | --- |
| **Render LOD** | per-client camera/focus | none (cosmetic) | dormant body → billboard; focused body → full terrain + units drawn |
| **Sim LOD** | global game state | **identical on all clients** | (see decision below) |

**Decision (locked): full-sim every colonized body.** All colonized bodies run their full unit sim on all clients regardless of focus. This is simpler and is viable because (a) the ground sim is cheap when idle and decoupled from rendering, and (b) expected colonized-body counts are modest. The expensive work (rendering 1000s of units, particles, terrain LOD) still happens only for the focused body — **sim everything cheaply, render one richly**.

> **Preserved seam (important).** Keep body simulation behind a `BodySim` trait with
> a single `tick()` entry point, and keep cross-body interaction restricted to the
> system layer (freight, ships). That costs nothing now but means a *deterministic*
> sim-LOD / auto-resolve tier for dormant bodies can be slotted in later **without**
> touching gameplay code, if body counts ever grow past the full-sim budget. Don't
> build it yet; don't preclude it.

**Fog of war** then falls out as a pure client-side render filter over the shared sim: enemy units exist in every client's simulation but are drawn only inside allied vision; friendly/allied units are always drawn. (Accepted tradeoff: as with all lockstep RTS, full state on every client means maphacks are *possible*; standard and acceptable.)

---

## 4. Orbital ephemeris

Each body carries Keplerian elements `(a, e, i, Ω, ω, M₀)`; moons' elements are relative to their planet (hierarchical). Position at tick `t` is **analytic propagation**, not integration:

1. Mean anomaly `M = M₀ + n·(t − t₀)`, mean motion `n = √(μ/a³)`.
2. Solve **Kepler's equation** `M = E − e·sin E` for eccentric anomaly `E` by Newton iteration ([johndcook](https://www.johndcook.com/blog/2022/11/01/kepler-newton/); 0–2 iterations suffice 99.996% of the time, [A&A 2022](https://www.aanda.org/articles/aa/full_html/2022/02/aa41423-21/aa41423-21.html)).
3. True anomaly → position in orbital plane → rotate by `ω, i, Ω` into the parent frame.

> **Why analytic, not RK4:** zero integration drift (cross-platform deterministic by
> construction) and O(1) per body per query. Kepler's equation can even be solved
> with **CORDIC** ([arXiv:2008.02894](https://arxiv.org/pdf/2008.02894)) — the same
> primitive as the fixed-point trig in the sim core — so the orbital layer drops
> straight into fixed-point with a bounded, deterministic iteration count.

Varying inclination/eccentricity (slight inter-planet `i`, eccentric moons) are just element values, and they create real strategy: plane changes and eccentric arrivals cost Δv, and transfer windows open and close.

---

## 5. Ship flight model

Ships are **patched conics** ([Wikipedia](https://en.wikipedia.org/wiki/Patched_conic_approximation), the KSP "on-rails" approach): a ship is bound to one dominant body's sphere of influence at a time; crossing an SOI boundary "patches" to the new body with position/velocity continuity. A per-ship **flight-mode state machine**:

```
            ┌─────────────┐  order: go to body/orbit   ┌──────────────┐
            │   COAST      │ ─────────────────────────► │  TRANSFER     │
            │ (Kepler rails│                            │ (Lambert ↔    │
            │  analytic)   │ ◄───── arrive ──────────── │ brachistochr.)│
            └─────┬────────┘                            └──────┬────────┘
       SOI cross  │                                  near target & matched
                  ▼                                            ▼
            ┌─────────────┐                            ┌──────────────┐
            │   PATCH      │                            │  ENGAGE       │
            │ (rebind SOI) │                            │ (CW relative  │
            └─────────────┘                            │  + steering)  │
                                                       └──────────────┘
              launch/land hand-off ⇄ surface layer (§2)
```

- **COAST / orbit** → analytic Kepler propagation around the current SOI body. Free, deterministic, on-rails.
- **TRANSFER** → the trajectory spectrum (§6).
- **ENGAGE** → close-proximity combat via Clohessy–Wiltshire relative motion (§7).
- **PATCH** → recompute orbital elements relative to the new SOI body.

High thrust is what makes all the simplifications valid: burns are short relative to orbit periods, so impulsive/continuous-thrust approximations hold.

### 5.1 The trajectory is real, not an animation

The curved path a ship follows is **actual simulated motion in the deterministic core**, not a cosmetic spline. Each ship carries `(position, velocity)` advanced every sim tick by one of two real-motion modes:

- **Coasting** → **analytic Kepler propagation** around the SOI body (§4). An exact conic — a genuine curved orbit, zero integration cost, zero drift.
- **Thrusting** (transfer/combat) → **fixed-point numerical integration** of `thrust + dominant-body gravity` per tick (semi-implicit Euler). The path curves because gravity is *in the loop*; the §6.2 guidance law steers thrust toward the planned arrival and self-corrects for it.

This is affordable for exactly the reason ground units aren't integrated this way: **ships are few** (a dozen, not thousands). Because the motion is real and deterministic, the trajectory *is* gameplay:

- you can **intercept** a ship mid-transfer — its future position is real and predictable;
- **fuel actually depletes** along the flown path (rocket equation on the real burn), so an under-fuelled ship falls short or ends on the wrong orbit — a real consequence, not a scripted outcome;
- **weapon range envelopes** read from the true relative geometry;
- every client computes the identical path (fixed-point lockstep), so it's authoritative, not client-side eye-candy.

> **What "animation" actually refers to:** only the *cosmetic dressing* on top of the
> real path — the visible 180° **flip** at the brachistochrone midpoint, the engine
> plume (bevy_hanabi) firing on accel/decel and dark during coast, radiator glow on
> hot burns. Orientation = the sim's thrust vector; these visuals are interpolated at
> 120–160 fps and drive nothing. The *drift* you want lives in the trajectory, not here.

### 5.2 Where the drift reads

Trajectory **shape becomes readable information**, because the physics — not an artist — draws it:

- **Efficient (Lambert) transfers** trace long, obvious **Kepler ellipses** (mostly coast) → "this ship is saving fuel / arriving late."
- **Brachistochrone torch runs** are comparatively **direct** (high thrust overpowers gravity over a short transit) → "this ship is burning hard / arriving fast." That contrast is honest *and* a gameplay tell.
- **Departure and arrival always curve** — spiralling out of the origin well, capturing into the destination orbit.
- **Combat always drifts** (Clohessy–Wiltshire, §7).
- In the **system view**, even a torch run curves in inertial space because both endpoint planets are moving; in the **local view** you see the departure/capture spirals and the combat drift.

A **KSP-style predicted-path overlay** (projected arc + flip point + the orbit it will capture into) is then just a *visualization of the real planned trajectory* — it shows where the ship will actually be, which is what sells "we're making actual transfers."

---

## 6. The trajectory spectrum: Lambert ↔ brachistochrone

**Decision (locked): build a unified planner spanning a Lambert (efficient) ↔ brachistochrone (fast) spectrum.** These are the two endpoints of one axis — trade fuel for time:

| | **Lambert (ballistic)** | **Brachistochrone (continuous thrust)** |
| --- | --- | --- |
| Profile | two impulsive burns, coast on a Kepler arc between | burn at full thrust to midpoint, **flip**, decelerate to arrive at rest |
| Optimizes | **min Δv** (fuel) | **min time** |
| Costs | long travel time | lots of reaction mass |
| Who uses it | freighters, fuel-conscious repositioning | warships, urgent intercepts ("torchship" runs) |
| Math | solve [Lambert's problem](https://en.wikipedia.org/wiki/Lambert%27s_problem) for a chosen time-of-flight (Izzo/universal-variable solver) | constant-acceleration flip-and-burn ([constant-accel travel](https://en.wikipedia.org/wiki/Space_travel_under_constant_acceleration), [torchships](https://www.projectrho.com/public_html/rocket/torchships.php)) |

That the brachistochrone is genuinely the *minimum-time* trajectory is not a hand-wave: under constant available thrust, Pontryagin's principle gives a **bang-bang** optimal control — max thrust throughout with a single direction switch (the flip) ([trajectory optimization](https://en.wikipedia.org/wiki/Trajectory_optimization)). Between the two endpoints lies a continuum of **burn–coast–burn** profiles; spending more Δv shortens the trip, smoothly interpolating from Lambert toward brachistochrone.

### 6.1 Closed-form brachistochrone (the planner's *estimate*)

These closed forms are the **planner's estimate** — used for the UI, the AI, and to set the guidance target — *not* the flown path (that is the real integrated trajectory of §5.1). Because thrust ≫ gravity, the estimate ignores gravity and charges gravity-well escape/capture as Δv overheads at the endpoints; the flown path keeps gravity and its guidance closes the gap. For a rest-to-rest hop over distance `d` at acceleration `a = F/m`:

```
t_flip = √(d / a)          T = 2·√(d / a)
v_peak = √(a · d)          Δv = a · T = 2·√(a · d)
```

`a` comes from the ship's engine + power loadout; available Δv comes from the rocket equation `Δv = v_e · ln(m₀/m_f)` ([Tsiolkovsky](https://en.wikipedia.org/wiki/Tsiolkovsky_rocket_equation)). All terms are arithmetic + `sqrt` → fixed-point/CORDIC deterministic.

### 6.2 Moving-target intercept

For a moving target (another ship, a body), there is no solver needed in the high-thrust regime — use a **closed-loop flip-and-burn guidance law**, which is exactly the cited behavior (*point at the lead position and burn; flip to decelerate*):

```
each tick:
  lead   = predict target state a short horizon ahead (analytic if on Kepler rails)
  toward = unit(lead.pos − self.pos)
  if stopping_distance(self.v_rel) ≥ distance_to(lead):  thrust = −unit(self.v_rel)   // flip & brake
  else:                                                  thrust = toward              // chase
```

This is emergent, allocation-free, and deterministic (no convergence loop). For *strategic* planning UI ("when do we arrive, what does it cost?"), close the loop offline with a small fixed-iteration root-find on flight time `T` against the target's analytic future position — bounded iterations, fixed-point safe.

### 6.3 The unified planner

Given (ship state, target trajectory, **urgency**), the planner:

1. Computes the efficient endpoint (Lambert / near-Hohmann min-Δv) and the fast endpoint (brachistochrone min-T).
2. Checks feasibility against the ship's **available Δv** (rocket equation) and **thrust** (loadout): a ship may be unable to afford the fast profile, forcing it toward coast-heavy profiles — or unable to make an intercept at all ("insufficient Δv").
3. Picks the fastest *feasible* profile that meets the order's urgency, or reports the constraint to the player.

Player-facing UI stays simple — "travel time + fuel cost," an urgency slider — while the model underneath is real. (This is the "Lambert under the hood" UX with the brachistochrone extreme added for warships.)

---

## 7. Close-combat: Clohessy–Wiltshire relative motion

**Decision (locked): orbital combat uses Hill / Clohessy–Wiltshire relative dynamics**, not gravity-free steering — engagements should retain authentic orbital *drift*.

When ships close to short range in similar orbits (the ENGAGE state), simulate their **relative** motion in the target's **LVLH frame** (x = radial-out, y = along-track, z = cross-track) with the linearized [Clohessy–Wiltshire equations](https://en.wikipedia.org/wiki/Clohessy%E2%80%93Wiltshire_equations):

```
ẍ − 3n²x − 2nẏ = a_x
ÿ + 2nẋ        = a_y
z̈ + n²z        = a_z          (n = mean motion of the reference orbit)
```

This is closed-form between burns (solutions are `sin/cos` of `n·t` → CORDIC trig, deterministic) and gives the counterintuitive, skill-rewarding feel: thrusting prograde makes you rise and fall behind; a clean intercept requires respecting the drift. High-thrust ships can fight it, but it's always present.

> **Validity envelope:** CW assumes small separation relative to orbit radius and a
> near-circular reference orbit. Inside that envelope (co-orbiting ships brawling near
> a planet) it's ideal. Outside it (wildly different orbits, hyperbolic flybys),
> fall back to the §6 translational regime. The flight-mode FSM switches ENGAGE↔TRANSFER
> on a separation/relative-velocity threshold.

---

## 8. The construction → mobility → combat loop

This is the spine that makes the realism *gameplay*. Ship components form a **power / heat / mass balance** that feeds directly into the trajectory and combat models:

```
   ┌────────────────── CONSTRUCTION ──────────────────┐
   │ reactor → power      generator → power (some need │
   │ radiator → heat sink  reactors)  thruster → thrust│
   │ weapon/prospecting platform (lasers/missiles/PDC) │
   └───────────────┬───────────────────────────────────┘
                   ▼  balance: power_out ≥ power_in, heat_out ≥ heat_in, mass = Σ parts
        ┌──────────────────────┐        a = F/m  (thrust accel)
        │  ENGINE PERFORMANCE   │        Δv = v_e·ln(m₀/m_f)  (rocket eq.)
        │  thrust accel a, Δv,  │────────────────────────────────────┐
        │  efficiency (Isp)     │                                    ▼
        └──────────┬────────────┘                       ┌────────────────────────┐
                   ▼                                     │ TRAJECTORY (§6)         │
        ┌──────────────────────┐   disable reactor       │ brachistochrone time =  │
        │ COMBAT (§9)           │   ⇒ power drops ⇒       │  2√(d/a); feasibility   │
        │ subsystem targeting:  │   a & weapons degrade   │  bounded by Δv          │
        │ radiator/reactor/gen  │◄───────────────────────►│ ⇒ where a ship can go   │
        └──────────────────────┘                         │   & combat reach        │
                                                         └─────────────────────────┘
```

- **Thruster tiers gate on power/heat:** self-contained (weak, solar-dependent) → generator-fed (medium) → reactor-fed (powerful; reactors may themselves need generators/radiators). Reactors/generators modulate efficiency (Isp) and thrust, which **extends Δv and therefore range and combat reach**.
- **Mass matters** (rocket equation): more parts → more mass → less Δv and lower `a` unless you scale propulsion. Ship design is a real optimization, not a checklist.
- **Combat closes the loop:** targeting an enemy's reactor cuts its power → lowers `a` and weapon output → lengthens its brachistochrone (`T = 2√(d/a)`) → it can't flee or intercept. Disable vs. capture vs. destroy become distinct outcomes against the same subsystem map.

---

## 9. Space combat

Few, expensive ships (≈ a dozen), so the sim can afford **depth per ship** (contrast ground: many, cheap). Characteristics:

- **Auto-engage in range**; player sets posture, targets, and weapon discipline.
- **Soft rock-paper-scissors:** missiles threaten hulls → **PDCs + beams** intercept missiles → heavy/smart missiles **saturate** point defense → beams out-range/out-track but are countered by hardening/range. Tuning these counters is the combat metagame.
- **Subsystem targeting:** ships are composite entities with located subsystems (radiator / reactor / generator / weapon / drive). Damage is resolved against a *simplified* geometric exposure model (subsystems at fixed local positions; incoming aspect + tracking → which subsystem is hit) — never mesh-level raycasts in the sim. Outcomes: **disable** (knock out a subsystem → §8 cascade), **capture** (board a powered-down hull), **destroy**.
- **Determinism:** projectiles/missiles are sim entities on fixed-point trajectories; beams are instantaneous fixed-point ray tests resolved at tick. No floats, no per-client variance.
- **Prospecting reuses the weapon system:** firing lasers/projectiles at an asteroid or moon in "survey" mode exposes/extracts resource data — the dual-use platform you described, one weapon sim with two modes.

---

## 10. Ground: colonization, prospecting, launch asymmetry

A logistics/production RTS where the **gravity well sets the rules**:

- **Prospecting & resources:** asteroids carry 1–2 specialized resources (revealed by prospecting, gated by prospector skill); planets auto-expose resource hotspots. Deterministic from body seed + reveal state.
- **Production graph:** land on a hotspot → refinery → factories → intermediate parts → rocket parts → spaceport/ships. A research tree gates recipes; what you can't build locally you **ship in via freighters** (the §6 transfer system → logistics has real timing/fuel cost).
- **Launch-infrastructure asymmetry is emergent, not authored.** Δv-to-orbit ∝ a body's gravity well (from its mass/radius), so:
  - **Asteroids** — shallow well → cheap launch → just a *launch site + ground-control facility*.
  - **Planets / large moons** — deep well → need a *full spaceport + heavy-lift* infrastructure, which is the laborious build chain (factories → rocket parts → heavy lift) you described.
- **Day/night & solar power (Q1 decision):** the focused planet's `R(t)`/`P(t)` drive lighting **and** solar-power availability — self-contained solar thrusters and bases vary output with day/night and orbital distance (eclipses behind the planet/moons dim them). Ground **unit physics stay in the stable rotating frame**; orbital state reaches gameplay only through power/lighting/launch-window timing, not unit motion.

---

## 11. Decisions locked & still open

**Locked (this review):**

1. **Orbit→ground reach:** lighting + day/night + **solar-power availability**; ground physics stay in the stable rotating frame. (Not full eclipse/sensor tactical coupling — for now.)
2. **Orbital combat:** **Clohessy–Wiltshire** relative dynamics in LVLH (authentic drift), with a validity-envelope fallback to translational steering.
3. **Transfers:** unified **Lambert ↔ brachistochrone** trajectory spectrum; simple "time + fuel" UI over a real model.
4. **Sim scaling:** **full-sim every colonized body**; keep the `BodySim` seam so deterministic sim-LOD can be added later if needed.

**Still open:**

- Reaction mass / fuel model: single "Δv tank," or distinct propellant + reactor-fuel resources? (Affects construction depth and the §8 loop.)
- Does solar power ever gate *combat* (e.g., solar-only ships near eclipse), or strictly economy/mobility?
- Boarding/capture rules: crew, time-to-capture, contest mechanics.
- CW combat: is a 2D orbital-plane simplification acceptable for readability, or full 3D LVLH?
- Where the surface↔orbit hand-off sits in the UX (seamless camera vs. an explicit "to orbit" transition).

---

## References

**Reference frames**
- Multiple/nested coordinate spaces — https://gamemath.com/book/multiplespaces.html
- NASA NAIF frames & coordinate systems — https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/Tutorials/pdf/individual_docs/17_frames_and_coordinate_systems.pdf

**Orbital mechanics**
- Patched conic approximation — https://en.wikipedia.org/wiki/Patched_conic_approximation
- Kepler's equation via Newton — https://www.johndcook.com/blog/2022/11/01/kepler-newton/ · fast solvers (0–2 iters) — https://www.aanda.org/articles/aa/full_html/2022/02/aa41423-21/aa41423-21.html · CORDIC Kepler — https://arxiv.org/pdf/2008.02894
- Lambert's problem — https://en.wikipedia.org/wiki/Lambert%27s_problem
- Orbital maneuver (high-thrust point-and-burn) — https://en.wikipedia.org/wiki/Orbital_maneuver
- Clohessy–Wiltshire equations — https://en.wikipedia.org/wiki/Clohessy%E2%80%93Wiltshire_equations

**Constant-thrust / brachistochrone**
- Space travel under constant acceleration — https://en.wikipedia.org/wiki/Space_travel_under_constant_acceleration
- Torchships (Atomic Rockets) — https://www.projectrho.com/public_html/rocket/torchships.php
- Trajectory optimization / bang-bang & Pontryagin — https://en.wikipedia.org/wiki/Trajectory_optimization

**Δv / propulsion**
- Tsiolkovsky rocket equation — https://en.wikipedia.org/wiki/Tsiolkovsky_rocket_equation
- Delta-v budget & porkchop plots — https://en.wikipedia.org/wiki/Delta-v_budget
