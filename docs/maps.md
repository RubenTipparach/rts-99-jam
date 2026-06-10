# Scenario map files

The starting layout of a battle (bases, garrisons, and every neutral resource
node) is baked into a human-readable `.map` file in `assets/maps/`, next to
the `.vxl` voxel terrain. The client embeds the file at compile time with
`include_str!` (so it loads identically native and on WASM) and
`apps/client/src/map.rs` parses it into ordinary setup `Command`s. Those flow
through the same deterministic sim path as everything else: the map file is
pure data, the sim never reads files.

The default skirmish scenario is `assets/maps/crossfire_basin.map`.

## Format

One entity per line. `#` starts a comment (full-line or trailing), blank
lines are ignored, and tokens are separated by whitespace.

```text
name <map name>                      # required, once
hq|barracks|turret     <player> <x> <z>
infantry|worker|heavy  <player> <x> <z>
ore|carbon             <x> <z>
```

- Coordinates are integer world units; the battlefield spans `-512..512` on
  both axes (`terrain::HALF`). The parser rejects anything outside it.
- `<player>` is the numeric player id (`0` = the human, `1` = the bot
  commander in the default skirmish). Which players are bot-driven is set in
  code (`World::set_bot`), not in the map.
- `ore` and `carbon` nodes are always neutral; they take no player id. Node
  capacities are sim constants (`node_capacity`), not map data, so all
  patches of a kind are worth the same.

A malformed map fails `Game::new` immediately (and the parser's unit tests in
`map.rs`, which validate the baked file on every `cargo test`).

## Placement rules of thumb

The parser does not enforce these, but `map.rs` has tests that check the
baked map against them:

- Keep resource nodes at least **8 units apart** center-to-center (their
  obstacle radius is 3.5, so that leaves a worker-width gap).
- Keep buildings at least **14 units** from any node (`BUILD_CLEAR2`), or
  workers can never raise a structure near the cluster.
- Mirror contested clusters left/right and top/bottom so no side has a
  shorter path to a shared expansion.

## Design guide (after StarCraft)

Crossfire Basin follows classic StarCraft ladder-map structure:

- **Classic starts.** Every player opens with exactly an **HQ and four
  workers** staged on the ore line (the Astromancer Spire or the Hollowmen
  Command HQ, per `docs/factions.md`). The HQ trains workers and is their
  deposit point; production buildings are constructed in-game by workers,
  never given for free.
- **Main bases.** Each main has a 6-patch **ore line arced behind the base**,
  between the base and the map edge, so the worker line sits in the most
  defensible pocket of the base; a single **carbon geyser caps one flank** of
  the line, like a gas geyser beside the mineral line.
- **Naturals.** Every main has a 4-ore + 1-carbon cluster one lane out
  toward the middle: the cheap second base that is easy to take and harder
  to hold.
- **Rich center.** The middle of the map is a 6-ore ring with **two**
  geysers, open ground in the center of the ring: the "gold expansion" that
  is worth fighting over precisely because every lane can reach it.
- **Side thirds.** A 4-ore + 1-carbon cluster hugs each of the east and west
  rims at mid-map, rewarding whoever controls the flanking routes.
- **Symmetry.** Shared clusters are mirrored across both axes, so the lone
  southern player and the two northern bot bases pay the same travel for any
  contested patch.

## Adding a map

1. Drop a new `.map` file in `assets/maps/` (copy the header comment from
   `crossfire_basin.map` as a template).
2. Embed and select it in `apps/client/src/map.rs` (a new `include_str!`
   constant) and point `Game::new` at it.
3. Run `cargo test -p client`: the parse/clearance tests will tell you if a
   cluster is malformed before you ever launch the game.
